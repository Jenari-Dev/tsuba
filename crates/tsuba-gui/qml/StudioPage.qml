import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// Lighting Studio: layers of effects on selected LEDs, rendered live by tsubad.
Rectangle {
    id: studio

    required property var backend
    required property string pid           // the keyboard the page opened with
    // Device being edited (each Chroma profile holds layers per device).
    property string editPid: pid
    readonly property bool editMouse: editPid === "00aa"
    readonly property var devicesHere: {
        const d = [{ pid: pid, name: "Keyboard" }];
        if (JSON.parse(backend.devices || "[]").some(x => x.pid === "00aa")) {
            d.push({ pid: "00aa", name: "Mouse" });
        }
        return d;
    }
    property var pack: packOf(editPid)
    function packOf(p) {
        const j = backend.device_pack(p);
        return j === "" ? null : JSON.parse(j);
    }
    // "All devices": keyboard and mouse side by side; the one last clicked is the one edited.
    property bool combined: false
    readonly property var kbPack: packOf(pid)
    readonly property var mousePack: packOf("00aa")
    readonly property bool hasMouse: devicesHere.length > 1
    readonly property var kbShown: combined ? kbPack : (editMouse ? null : pack)
    readonly property var mouseShown: combined ? mousePack : (editMouse ? pack : null)
    property var liveByPid: ({})
    readonly property var tabNames: devicesHere.map(d => d.name).concat(hasMouse ? ["All devices"] : [])
    readonly property var mouseLeds: [{ led: [0, 0], name: "Logo" }, { led: [0, 1], name: "Scroll wheel" }].concat(Array.from({ length: 11 }, (_, i) => ({ led: [0, 2 + i], name: "Underglow" })))
    readonly property var leds: editMouse ? mouseLeds : (pack ? pack.views.top.hotspots.filter(h => h.led !== null) : [])
    // Underglow runs 2 (front-left) → 7 (back) → 12 (front-right).
    readonly property var mouseGroups: [
        { label: "All", test: h => true },
        { label: "Logo", test: h => h.led[1] === 0 },
        { label: "Scroll wheel", test: h => h.led[1] === 1 },
        { label: "All underglow", test: h => h.led[1] >= 2 },
        { label: "Left side", test: h => h.led[1] >= 2 && h.led[1] <= 6 },
        { label: "Back", test: h => h.led[1] >= 6 && h.led[1] <= 8 },
        { label: "Right side", test: h => h.led[1] >= 8 },
        { label: "Logo + wheel", test: h => h.led[1] <= 1 }
    ]

    // Named Chroma profiles.
    property var profile: ({ name: "My Lighting", devices: {} })
    property string loadedName: ""
    property var profileNames: []
    property string activeName: ""
    property string status: ""
    property var liveColours: ({})

    // Model: list of layers, index 0 = bottom. The UI lists them top-first.
    property var layers: []
    property int current: -1
    property bool dirty: false
    property int editingColour: 0
    property bool cleared: false        // selection shown empty until the user picks LEDs

    // Undo / redo: snapshots (JSON) of `layers`.
    property var history: []
    property int hpos: -1
    property double lastSnap: 0
    readonly property bool canUndo: hpos > 0
    readonly property bool canRedo: hpos >= 0 && hpos < history.length - 1
    function resetHistory() {
        history = [JSON.stringify(layers)];
        hpos = 0;
    }
    function snapshot() {
        const now = Date.now();
        const h = history.slice(0, hpos + 1);
        const js = JSON.stringify(layers);
        // Drags (sliders, colour area) fire many edits: merge those within 400 ms.
        if (h.length > 1 && now - lastSnap < 400) {
            h[h.length - 1] = js;
        } else if (h[h.length - 1] !== js) {
            h.push(js);
        }
        lastSnap = now;
        history = h.slice(-100);
        hpos = history.length - 1;
    }
    function restore(i) {
        hpos = i;
        layers = JSON.parse(history[i]);
        current = Math.min(current, layers.length - 1);
        cleared = false;
        dirty = true;
        push.restart();
        restartPreview();
    }
    function undo() {
        if (canUndo) {
            restore(hpos - 1);
        }
    }
    function redo() {
        if (canRedo) {
            restore(hpos + 1);
        }
    }
    onCurrentChanged: {
        cleared = false;
        editingColour = 0;
    }

    readonly property var effects: [
        { type: "static", label: "Static", colours: 1 },
        { type: "breathing", label: "Breathing", colours: 2, random: true },
        { type: "spectrum", label: "Spectrum Cycling", colours: 0 },
        { type: "wave", label: "Wave", colours: 2, random: true, directions: ["Left to right", "Right to left", "Top to bottom", "Bottom to top"] },
        { type: "wheel", label: "Wheel", colours: 0, directions: ["Clockwise", "Counter-clockwise"] },
        { type: "starlight", label: "Starlight", colours: 2, random: true, density: true },
        { type: "fire", label: "Fire", colours: 0 },
        { type: "audio_meter", label: "Audio Meter", colours: 0 },
        { type: "ambient", label: "Ambient (screen)", colours: 0 },
        { type: "tidal", label: "Tidal", colours: 0, directions: ["Outward", "Inward"] },
        { type: "battery", label: "Battery level", colours: 0 }
    ]
    readonly property var dirKeys: ({
        wave: ["left_to_right", "right_to_left", "top_to_bottom", "bottom_to_top"],
        wheel: ["clockwise", "counter_clockwise"]
    })
    readonly property var groups: [
        { label: "All", test: h => true },
        { label: "WASD", test: h => (h.group || "").indexOf("wasd") >= 0 },
        { label: "Arrows", test: h => (h.group || "").indexOf("arrows") >= 0 },
        { label: "Letters", test: h => (h.group || "").indexOf("alphabets") >= 0 },
        { label: "Number row", test: h => (h.group || "").indexOf("numberRows") >= 0 },
        { label: "F-keys", test: h => (h.group || "").indexOf("functions") >= 0 },
        { label: "Numpad", test: h => (h.group || "").indexOf("numpad") >= 0 },
        { label: "Macro", test: h => h.group === "macro" },
        { label: "Media", test: h => (h.group || "").indexOf("multimedia") >= 0 },
        { label: "Underglow", test: h => h.group === "underglow" }
    ]

    color: Theme.bg

    // Small square toolbar button with a hand-drawn glyph.
    component ToolBtn: Rectangle {
        id: tb
        property string kind: "all"
        property string tip: ""
        signal clicked
        implicitWidth: 28
        implicitHeight: 24
        width: 28
        height: 24
        radius: 3
        color: th.hovered && enabled ? "#26ffffff" : "transparent"
        border.color: th.hovered && enabled ? Theme.accent : Theme.borderSoft
        opacity: enabled ? 1 : 0.35
        readonly property color ink: th.hovered && enabled ? Theme.accent : Theme.text
        onInkChanged: cv.requestPaint()
        Canvas {
            id: cv
            anchors.centerIn: parent
            width: 16
            height: 16
            onPaint: {
                const c = getContext("2d");
                c.reset();
                c.strokeStyle = tb.ink;
                c.lineWidth = 1.5;
                c.lineCap = "round";
                c.lineJoin = "round";
                const ln = (a, b, d, e) => {
                    c.beginPath();
                    c.moveTo(a, b);
                    c.lineTo(d, e);
                    c.stroke();
                };
                switch (tb.kind) {
                case "all":      // dashed selection box with a tick
                    c.setLineDash([2, 2]);
                    c.strokeRect(2, 2, 12, 12);
                    c.setLineDash([]);
                    ln(5, 8, 7, 10);
                    ln(7, 10, 11, 5.5);
                    break;
                case "clear":    // dashed box with a cross
                    c.setLineDash([2, 2]);
                    c.strokeRect(2, 2, 12, 12);
                    c.setLineDash([]);
                    ln(5.5, 5.5, 10.5, 10.5);
                    ln(10.5, 5.5, 5.5, 10.5);
                    break;
                case "undo":
                case "redo": {
                    const f = tb.kind === "redo" ? -1 : 1;
                    const X = x => f === 1 ? x : 16 - x;
                    c.beginPath();
                    c.moveTo(X(4), 6);
                    c.lineTo(X(10), 6);
                    c.bezierCurveTo(X(14), 6, X(14), 13, X(10), 13);
                    c.lineTo(X(6), 13);
                    c.stroke();
                    ln(X(4), 6, X(7), 3);
                    ln(X(4), 6, X(7), 9);
                    break;
                }
                case "minus":
                    ln(4, 8, 12, 8);
                    break;
                case "plus":
                    ln(4, 8, 12, 8);
                    ln(8, 4, 8, 12);
                    break;
                }
            }
        }
        HoverHandler {
            id: th
            cursorShape: Qt.PointingHandCursor
        }
        TapHandler {
            enabled: tb.enabled
            onTapped: tb.clicked()
        }
        QQC2.ToolTip.visible: th.hovered && tip !== ""
        QQC2.ToolTip.text: tip
        QQC2.ToolTip.delay: 500
    }

    function effectInfo(type) {
        return effects.find(e => e.type === type) || effects[0];
    }
    function newEffect(type) {
        switch (type) {
        case "static":
            return { type: type, colour: "44d62c" };
        case "breathing":
            return { type: type, colours: ["44d62c"], duration: "medium" };
        case "spectrum":
            return { type: type, duration: "medium" };
        case "wave":
            return { type: type, direction: "left_to_right", duration: "medium", colours: [] };
        case "wheel":
            return { type: type, direction: "clockwise", duration: "medium" };
        case "starlight":
            return { type: type, colours: [], duration: "medium", density: 0.15 };
        case "audio_meter":
            return { type: type };
        case "ambient":
            return { type: type, boost: 1.3 };
        case "tidal":
            return { type: type, inward: false, duration: "medium", colours: [] };
        case "battery":
            return { type: type };
        default:
            return { type: type, duration: "medium" };
        }
    }
    function addLayer(type) {
        const l = layers.slice();
        l.push({ name: effectInfo(type).label, enabled: true, leds: [], effect: newEffect(type) });
        layers = l;
        current = l.length - 1;
        changed();
    }
    function updateCurrent(fn) {
        if (current < 0) {
            return;
        }
        const l = JSON.parse(JSON.stringify(layers));
        fn(l[current]);
        layers = l;
        changed();
    }
    function moveLayer(i, d) {
        const j = i + d;
        if (j < 0 || j >= layers.length) {
            return;
        }
        const l = layers.slice();
        [l[i], l[j]] = [l[j], l[i]];
        layers = l;
        current = j;
        changed();
    }
    function removeLayer(i) {
        const l = layers.slice();
        l.splice(i, 1);
        layers = l;
        current = Math.min(current, l.length - 1);
        changed();
    }
    function changed() {
        snapshot();
        dirty = true;
        push.restart();
        restartPreview();
    }
    function restartPreview() {
        backend.preview_start(editPid, JSON.stringify({ layers: layers }));
    }
    // Start the live preview of the devices that are not being edited (combined view), then
    // re-start the edited one last so preview_frame() keeps meaning "the edited device".
    function startOtherPreviews() {
        devicesHere.forEach(d => {
            if (d.pid !== editPid) {
                const st = profile.devices[d.pid];
                backend.preview_start(d.pid, JSON.stringify({ layers: st ? st.layers : [] }));
            }
        });
        restartPreview();
    }
    // LEDs picked on one device of the combined view: make it the edited device, then select.
    function pickLeds(devPid, sel) {
        if (devPid !== editPid) {
            switchDevice(devPid);
        }
        cleared = false;
        updateCurrent(l => l.leds = Object.keys(sel).map(k => k.split(",").map(Number)));
    }
    function showCombined(on) {
        combined = on && hasMouse;
        if (combined) {
            stash();
            startOtherPreviews();
        }
    }
    function stash() {
        const d = Object.assign({}, profile.devices);
        d[editPid] = { layers: layers };
        profile = Object.assign({}, profile, { devices: d });
    }
    function switchDevice(p) {
        if (p === editPid) {
            return;
        }
        stash();
        editPid = p;
        const st = profile.devices[p];
        layers = st ? JSON.parse(JSON.stringify(st.layers)) : [];
        current = layers.length - 1;
        resetHistory();
        restartPreview();
    }
    function openProfile(p) {
        profile = JSON.parse(JSON.stringify(p));
        loadedName = p.name;
        const st = profile.devices[editPid];
        layers = st ? JSON.parse(JSON.stringify(st.layers)) : [];
        current = layers.length - 1;
        dirty = false;
        resetHistory();
        startOtherPreviews();
    }
    function loadProfiles() {
        const j = JSON.parse(backend.chroma_profiles() || "{\"profiles\": []}");
        profileNames = j.profiles.map(p => p.name);
        activeName = j.active || "";
        return j;
    }
    function saveProfile(apply) {
        stash();
        if (loadedName !== "" && loadedName !== profile.name && profileNames.indexOf(loadedName) >= 0) {
            if (!backend.rename_chroma_profile(loadedName, profile.name)) {
                status = "Failed: " + backend.status;
                return;
            }
        }
        if (backend.save_chroma_profile(JSON.stringify(profile), apply)) {
            loadedName = profile.name;
            dirty = false;
            status = apply ? "Saved and applied" : "Saved";
        } else {
            status = "Failed: " + backend.status;
        }
        loadProfiles();
        Theme.profilesRev++;
    }
    function uniqueName(base) {
        let n = base, i = 2;
        while (profileNames.indexOf(n) >= 0) {
            n = base + " " + i++;
        }
        return n;
    }
    function studioJson() {
        return JSON.stringify({ type: "studio", studio: { layers: layers } });
    }
    function applyGroup(g) {
        const all = editMouse ? mouseGroups : groups;
        const grp = all.find(x => x.label === g);
        if (!grp) {
            return;
        }
        cleared = false;
        updateCurrent(l => l.leds = g === "All" ? [] : leds.filter(grp.test).map(h => h.led));
    }
    function editedColour() {
        const l = current >= 0 ? layers[current] : null;
        if (!l) {
            return "";
        }
        if (l.effect.type === "static") {
            return l.effect.colour || "44d62c";
        }
        const c = l.effect.colours || [];
        return c[Math.min(editingColour, c.length - 1)] || "";
    }
    function setColour(hex) {
        updateCurrent(l => {
            if (l.effect.type === "static") {
                l.effect.colour = hex;
            } else {
                const c = (l.effect.colours || []).slice();
                if (c.length === 0) {
                    return;
                }
                c[Math.min(editingColour, c.length - 1)] = hex;
                l.effect.colours = c;
            }
        });
    }
    function swatch(e) {
        if (e.colour) {
            return "#" + e.colour;
        }
        if (e.colours && e.colours.length) {
            return "#" + e.colours[0];
        }
        return e.type === "fire" ? "#ff7a1a" : "transparent";
    }
    readonly property var selection: {
        const s = {};
        if (!cleared && current >= 0 && layers[current]) {
            const l = layers[current];
            (l.leds.length ? l.leds : leds.map(h => h.led)).forEach(rc => s[rc[0] + "," + rc[1]] = true);
        }
        return s;
    }

    function load() {
        const j = loadProfiles();
        const act = j.profiles.find(p => p.name === j.active) || j.profiles[0];
        if (act) {
            openProfile(act);
            return;
        }
        const cfg = JSON.parse(backend.config(pid) || "{}");
        const e = cfg.lighting ? cfg.lighting.effect : null;
        if (e && e.type === "studio") {
            layers = e.studio.layers;
            current = layers.length - 1;
        } else if (backend.env_var("TSUBA_DEMO") === "1") {
            const sel = g => leds.filter(g.test).map(h => h.led);
            layers = [
                { name: "Wave", enabled: true, leds: [], effect: newEffect("wave") },
                { name: "Underglow stars", enabled: true, leds: sel(groups[9]), effect: { type: "starlight", colours: ["ffffff"], duration: "medium", density: 0.2 } },
                { name: "WASD", enabled: true, leds: sel(groups[1]), effect: { type: "static", colour: "ff2020" } }
            ];
            current = 2;
        }
        // First run: the device's current studio becomes the first profile.
        const devs = {};
        devs[pid] = { layers: layers };
        const mcfg = JSON.parse(backend.config("00aa") || "{}");
        if (mcfg.lighting && mcfg.lighting.effect && mcfg.lighting.effect.type === "studio") {
            devs["00aa"] = { layers: mcfg.lighting.effect.studio.layers };
        }
        profile = { name: "My Lighting", devices: devs };
        loadedName = "";
        dirty = layers.length > 0;
        resetHistory();
        startOtherPreviews();
    }

    Component.onCompleted: {
        load();
        if (backend.env_var("TSUBA_SYNAPSE") !== "") {   // headless UI check
            synapseDlg.open("lighting");
        }
        if (backend.env_var("TSUBA_STUDIO_DEVICE") !== "") {   // headless UI check
            if (backend.env_var("TSUBA_STUDIO_DEVICE") === "all") {
                showCombined(true);
            } else {
                switchDevice(backend.env_var("TSUBA_STUDIO_DEVICE"));
            }
        }
        if (backend.env_var("TSUBA_DEMO_EFFECT") !== "" && current >= 0) {   // headless UI check
            const fx = newEffect(backend.env_var("TSUBA_DEMO_EFFECT"));
            updateCurrent(l => l.effect = fx);
        }
    }

    Timer {
        id: push
        interval: 150
        onTriggered: if (!studio.backend.readonly && studio.layers.length) studio.backend.set_effect(studio.editPid, studio.studioJson(), 255)
    }

    // Live animated preview from Tsuba's own renderer (same code as the device output).
    Timer {
        interval: 40
        repeat: true
        running: studio.visible
        onTriggered: {
            if (studio.combined) {
                const m = {};
                studio.devicesHere.forEach(d => m[d.pid] = JSON.parse(studio.backend.preview_frame_for(d.pid) || "{}"));
                studio.liveByPid = m;
                studio.liveColours = m[studio.editPid] || {};
            } else {
                studio.liveColours = JSON.parse(studio.backend.preview_frame() || "{}");
            }
        }
    }

    Shortcut {
        sequence: "Ctrl+Z"
        enabled: studio.visible
        onActivated: studio.undo()
    }
    Shortcut {
        sequences: ["Ctrl+Shift+Z", "Ctrl+Y"]
        enabled: studio.visible
        onActivated: studio.redo()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // Header
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 56
            color: Theme.bg
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 28
                anchors.rightMargin: 24
                spacing: 14
                Text {
                    text: "LIGHTING STUDIO"
                    color: Theme.accent
                    font.family: Theme.titleFont
                    font.pixelSize: 16
                }
                // Profile: pick / name / new / duplicate / delete / save & apply
                SynDropdown {
                    Layout.preferredWidth: 200
                    model: studio.profileNames.length ? studio.profileNames : [studio.profile.name]
                    currentIndex: Math.max(0, studio.profileNames.indexOf(studio.loadedName))
                    onActivated: i => {
                        const j = studio.loadProfiles();
                        const p = j.profiles.find(x => x.name === studio.profileNames[i]);
                        if (p) {
                            studio.openProfile(p);
                            // Picking a profile switches the devices to it (like Synapse).
                            if (!studio.backend.readonly) {
                                studio.status = studio.backend.apply_chroma_profile(p.name) ? "Applied " + p.name : "Failed: " + studio.backend.status;
                                studio.loadProfiles();
                            }
                        }
                    }
                }
                QQC2.TextField {
                    Layout.preferredWidth: 180
                    text: studio.profile.name
                    placeholderText: "Profile name"
                    onTextEdited: {
                        studio.profile = Object.assign({}, studio.profile, { name: text });
                        studio.dirty = true;
                    }
                }
                Text {
                    visible: studio.loadedName !== "" && studio.loadedName === studio.activeName
                    text: "ACTIVE"
                    color: Theme.accent
                    font.family: Theme.font
                    font.pixelSize: 10
                    font.letterSpacing: 1
                }
                SynButton {
                    text: "New"
                    implicitWidth: 64
                    onClicked: {
                        studio.profile = { name: studio.uniqueName("Profile"), devices: {} };
                        studio.loadedName = "";
                        studio.layers = [];
                        studio.current = -1;
                        studio.dirty = true;
                        studio.resetHistory();
                        studio.restartPreview();
                    }
                }
                SynButton {
                    objectName: "studioSynapseImport"
                    text: "Import"
                    implicitWidth: 80
                    onClicked: synapseDlg.open("lighting")
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.text: "Import Chroma Studio effects from Synapse…"
                }
                SynButton {
                    text: "Duplicate"
                    implicitWidth: 96
                    onClicked: {
                        studio.stash();
                        studio.profile = Object.assign({}, JSON.parse(JSON.stringify(studio.profile)), { name: studio.uniqueName(studio.profile.name + " copy") });
                        studio.loadedName = "";
                        studio.dirty = true;
                    }
                }
                SynButton {
                    text: "Delete"
                    implicitWidth: 76
                    enabled: studio.loadedName !== "" && !studio.backend.readonly
                    onClicked: {
                        studio.backend.delete_chroma_profile(studio.loadedName);
                        Theme.profilesRev++;
                        const j = studio.loadProfiles();
                        if (j.profiles.length) {
                            studio.openProfile(j.profiles[0]);
                        } else {
                            studio.profile = { name: "My Lighting", devices: {} };
                            studio.loadedName = "";
                            studio.layers = [];
                            studio.resetHistory();
                        }
                    }
                }
                Item {
                    Layout.fillWidth: true
                }
                Text {
                    text: studio.status !== "" ? studio.status : (studio.backend.readonly ? "read-only daemon" : (studio.layers.length ? "LIVE ON DEVICE" : ""))
                    color: studio.status.startsWith("Failed") || studio.backend.readonly ? Theme.warn : Theme.accent
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    font.letterSpacing: 1
                }
                SynButton {
                    text: "Save & apply"
                    primary: studio.dirty
                    enabled: !studio.backend.readonly && studio.profile.name.trim() !== ""
                    onClicked: studio.saveProfile(true)
                }
            }
        }
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 2
            color: Theme.line
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            // ---- Layers
            Rectangle {
                Layout.preferredWidth: 270
                Layout.fillHeight: true
                color: Theme.panel
                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 16
                    spacing: 10
                    RowLayout {
                        Text {
                            text: "EFFECT LAYERS"
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                            font.letterSpacing: 0.5
                        }
                        HelpButton {
                            Layout.leftMargin: 8
                            size: 16
                            text: "A Lighting Studio profile is a stack of effect layers drawn on your devices' LEDs, top layer first. Add layers with +, choose which LEDs each one covers, and use Blend to decide how it mixes with the layers below. Profiles play on all devices while Tsuba runs."
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                        SynButton {
                            text: "+"
                            implicitWidth: 36
                            onClicked: addMenu.open()
                            QQC2.Menu {
                                id: addMenu
                                y: parent.height
                                Repeater {
                                    model: studio.effects
                                    delegate: QQC2.MenuItem {
                                        required property var modelData
                                        text: modelData.label
                                        onTriggered: studio.addLayer(modelData.type)
                                    }
                                }
                            }
                        }
                    }
                    ListView {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        spacing: 6
                        clip: true
                        // Show the top layer first.
                        model: studio.layers.length
                        delegate: Rectangle {
                            required property int index
                            readonly property int li: studio.layers.length - 1 - index
                            readonly property var lyr: studio.layers[li]
                            width: ListView.view.width
                            height: 52
                            color: studio.current === li ? Theme.panelRaised : (lh.hovered ? "#262626" : "transparent")
                            border.color: studio.current === li ? Theme.accent : "transparent"
                            RowLayout {
                                anchors.fill: parent
                                anchors.leftMargin: 10
                                anchors.rightMargin: 6
                                spacing: 8
                                Rectangle {
                                    // visibility toggle
                                    Layout.preferredWidth: 18
                                    Layout.preferredHeight: 18
                                    radius: 3
                                    color: lyr && lyr.enabled ? Theme.accent : "transparent"
                                    border.color: Theme.border
                                    TapHandler {
                                        onTapped: {
                                            studio.current = li;
                                            studio.updateCurrent(l => l.enabled = !l.enabled);
                                        }
                                    }
                                }
                                Item {
                                    Layout.preferredWidth: 24
                                    Layout.preferredHeight: 24
                                    EffectIcon {
                                        anchors.centerIn: parent
                                        size: 22
                                        type: lyr ? lyr.effect.type : "static"
                                        active: studio.current === li
                                    }
                                    Rectangle {
                                        // colour dot
                                        visible: lyr && studio.swatch(lyr.effect) !== "transparent"
                                        x: 14
                                        y: 14
                                        width: 9
                                        height: 9
                                        radius: 5
                                        color: lyr ? studio.swatch(lyr.effect) : "transparent"
                                        border.color: Theme.panel
                                        border.width: 1
                                    }
                                }
                                Column {
                                    Layout.fillWidth: true
                                    Text {
                                        text: lyr ? lyr.name : ""
                                        color: studio.current === li ? Theme.accent : Theme.text
                                        font.family: Theme.font
                                        font.pixelSize: Theme.fontSize
                                        elide: Text.ElideRight
                                        width: parent.width
                                    }
                                    Text {
                                        text: lyr ? studio.effectInfo(lyr.effect.type).label + " · " + (lyr.leds.length ? lyr.leds.length + " LEDs" : "all LEDs") : ""
                                        color: Theme.textDim
                                        font.family: Theme.font
                                        font.pixelSize: 11
                                    }
                                }
                                Column {
                                    Text {
                                        text: "▲"
                                        color: Theme.textDim
                                        font.pixelSize: 10
                                        TapHandler {
                                            onTapped: studio.moveLayer(li, 1)
                                        }
                                    }
                                    Text {
                                        text: "▼"
                                        color: Theme.textDim
                                        font.pixelSize: 10
                                        TapHandler {
                                            onTapped: studio.moveLayer(li, -1)
                                        }
                                    }
                                }
                                Text {
                                    text: "✕"
                                    color: Theme.textDim
                                    font.pixelSize: 13
                                    TapHandler {
                                        onTapped: studio.removeLayer(li)
                                    }
                                }
                            }
                            HoverHandler {
                                id: lh
                            }
                            TapHandler {
                                onTapped: studio.current = li
                            }
                        }
                    }
                    Text {
                        visible: studio.layers.length === 0
                        Layout.fillWidth: true
                        text: "Add a layer to start. Upper layers draw over lower ones."
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        wrapMode: Text.Wrap
                    }
                    Text {
                        Layout.topMargin: 6
                        text: "ADD EFFECT"
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        font.letterSpacing: 0.5
                    }
                    GridLayout {
                        Layout.fillWidth: true
                        columns: 3
                        rowSpacing: 8
                        columnSpacing: 4
                        Repeater {
                            model: studio.effects
                            delegate: Item {
                                required property var modelData
                                Layout.fillWidth: true
                                Layout.preferredWidth: 1
                                Layout.preferredHeight: 62
                                Rectangle {
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    y: 0
                                    width: 36
                                    height: 36
                                    radius: 18
                                    color: "transparent"
                                    border.color: eh.hovered ? Theme.accent : Theme.border
                                    border.width: 1
                                    EffectIcon {
                                        anchors.centerIn: parent
                                        size: 22
                                        type: modelData.type
                                        active: eh.hovered
                                    }
                                }
                                Text {
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    y: 40
                                    width: parent.width
                                    horizontalAlignment: Text.AlignHCenter
                                    text: modelData.label
                                    color: eh.hovered ? Theme.accent : Theme.text
                                    font.family: Theme.font
                                    font.pixelSize: 10
                                    elide: Text.ElideRight
                                }
                                HoverHandler {
                                    id: eh
                                    cursorShape: Qt.PointingHandCursor
                                }
                                TapHandler {
                                    onTapped: studio.addLayer(modelData.type)
                                }
                            }
                        }
                    }
                }
            }

            // ---- Device + LED selection
            Rectangle {
                Layout.fillWidth: true
                Layout.fillHeight: true
                color: "#3a3a3a"
            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 20
                spacing: 12
                ModeTabs {
                    visible: studio.devicesHere.length > 1
                    minTabWidth: 120
                    model: studio.tabNames
                    currentIndex: studio.combined ? studio.devicesHere.length : Math.max(0, studio.devicesHere.findIndex(d => d.pid === studio.editPid))
                    onActivated: i => {
                        if (i >= studio.devicesHere.length) {
                            studio.showCombined(true);
                        } else {
                            studio.combined = false;
                            studio.switchDevice(studio.devicesHere[i].pid);
                        }
                    }
                }
                // Toolbar: quick select, select all / clear, undo / redo.
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    enabled: studio.current >= 0
                    Text {
                        text: "Quick select"
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                    }
                    SynDropdown {
                        objectName: "quickSelect"
                        implicitWidth: 170
                        implicitHeight: 30
                        model: (studio.editMouse ? studio.mouseGroups : studio.groups).map(g => g.label)
                        currentIndex: -1
                        displayText: "Choose group…"
                        onActivated: i => {
                            studio.applyGroup(model[i]);
                            currentIndex = -1;
                        }
                    }
                    ToolBtn {
                        objectName: "selectAll"
                        kind: "all"
                        tip: "Select all LEDs"
                        onClicked: {
                            studio.cleared = false;
                            studio.updateCurrent(l => l.leds = []);
                        }
                    }
                    ToolBtn {
                        objectName: "clearSelection"
                        kind: "clear"
                        tip: "Clear selection"
                        onClicked: studio.cleared = true
                    }
                    Item {
                        Layout.fillWidth: true
                    }
                    ToolBtn {
                        objectName: "undoBtn"
                        kind: "undo"
                        tip: "Undo (Ctrl+Z)"
                        enabled: studio.canUndo
                        onClicked: studio.undo()
                    }
                    ToolBtn {
                        objectName: "redoBtn"
                        kind: "redo"
                        tip: "Redo (Ctrl+Shift+Z)"
                        enabled: studio.canRedo
                        onClicked: studio.redo()
                    }
                }
                Item {
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    Layout.fillHeight: true
                ZoomCanvas {
                    id: canvasHolder
                    objectName: "studioCanvas"
                    anchors.fill: parent
                    // Rubber band: forwarded to the device under the band's centre (combined view)
                    // or to the one device shown.
                    function target(r) {
                        if (!studio.combined) {
                            return studio.editMouse ? mouseView : kbView;
                        }
                        const c = Qt.point(r.x + r.width / 2, r.y + r.height / 2);
                        return c.x >= mouseHolder.x ? mouseView : kbView;
                    }
                    onBoxSelected: (r, add) => {
                        const t = target(r);
                        const f = t.mapFromItem(canvasHolder.contentLayer, r.x, r.y);
                        t.selectBox(Qt.rect(f.x, f.y, r.width, r.height), add);
                    }
                    onTapped: (p, add) => {
                        const t = target(Qt.rect(p.x, p.y, 0, 0));
                        t.selectAt(t.mapFromItem(canvasHolder.contentLayer, p.x, p.y), add);
                    }
                    LedKeyboard {
                        id: kbView
                        visible: studio.combined || !studio.editMouse
                        x: 0
                        y: 0
                        width: studio.combined ? Math.round(parent.width * 0.68) : parent.width
                        height: parent.height
                        interactive: false
                        view: studio.kbShown ? studio.kbShown.views.top : null
                        dir: studio.kbShown ? studio.kbShown.dir : ""
                        colours: studio.combined ? (studio.liveByPid[studio.pid] || {}) : studio.liveColours
                        selection: studio.combined && studio.editMouse ? ({}) : studio.selection
                        onSelectionEdited: sel => studio.pickLeds(studio.pid, sel)
                    }
                    Item {
                        id: mouseHolder
                        visible: studio.combined || studio.editMouse
                        x: studio.combined ? kbView.width : 0
                        y: 0
                        width: studio.combined ? parent.width - kbView.width : parent.width
                        height: parent.height
                        MouseLeds {
                            id: mouseView
                            anchors.fill: parent
                            interactive: false
                            view: studio.mouseShown ? studio.mouseShown.views.top : null
                            dir: studio.mouseShown ? studio.mouseShown.dir : ""
                            colours: studio.combined ? (studio.liveByPid["00aa"] || {}) : studio.liveColours
                            selection: studio.combined && !studio.editMouse ? ({}) : studio.selection
                            onSelectionEdited: sel => studio.pickLeds("00aa", sel)
                        }
                    }
                }
                Text {
                    visible: studio.editMouse || studio.combined
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.margins: 4
                    width: parent.width - 220
                    wrapMode: Text.Wrap
                    text: studio.current >= 0 ? "Click LEDs or drag a box around them to choose where this layer lights. Shift or Ctrl adds to the selection." : "Add or pick a layer, then choose its LEDs here."
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                }
                }
                Flow {
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    spacing: 6
                    enabled: studio.current >= 0
                    opacity: enabled ? 1 : 0.4
                    SectionLabel {
                        caption: "Layer LEDs"
                        width: 90
                        height: 28
                        verticalAlignment: Text.AlignVCenter
                    }
                    Repeater {
                        model: studio.editMouse ? studio.mouseGroups : studio.groups
                        delegate: Rectangle {
                            required property var modelData
                            width: gl.implicitWidth + 22
                            height: 28
                            color: gh.hovered ? "#1affffff" : Theme.panel
                            border.color: Theme.borderSoft
                            Text {
                                id: gl
                                anchors.centerIn: parent
                                text: modelData.label
                                color: Theme.text
                                font.family: Theme.font
                                font.pixelSize: Theme.small
                            }
                            HoverHandler {
                                id: gh
                            }
                            TapHandler {
                                onTapped: studio.applyGroup(modelData.label)
                            }
                        }
                    }
                }
            }
            }

            // ---- Effect settings
            Rectangle {
                id: fx
                Layout.preferredWidth: 340
                clip: true
                Layout.fillHeight: true
                color: Theme.panel
                visible: studio.current >= 0 && studio.layers[studio.current] !== undefined
                readonly property var cur: studio.layers[studio.current]
                readonly property var info: cur ? studio.effectInfo(cur.effect.type) : null
                readonly property int slots: !cur ? 0 : (cur.effect.type === "static" ? 1 : (cur.effect.colours || []).length)
                readonly property bool isMeter: !!cur && cur.effect.type === "audio_meter"
                readonly property bool isAmbient: !!cur && cur.effect.type === "ambient"
                Flickable {
                    id: fxFlick
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.bottom: saveBar.top
                    clip: true
                    contentWidth: width
                    contentHeight: fxCol.implicitHeight + 36
                    boundsBehavior: Flickable.StopAtBounds
                    QQC2.ScrollBar.vertical: QQC2.ScrollBar {}
                ColumnLayout {
                    id: fxCol
                    x: 18
                    y: 18
                    width: fxFlick.width - 36
                    spacing: 12
                    SectionLabel {
                        caption: "Layer brightness"
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 10
                        SynSlider {
                            objectName: "layerBrightness"
                            Layout.fillWidth: true
                            from: 0
                            to: 100
                            stepSize: 1
                            value: fx.cur && fx.cur.brightness !== undefined ? Math.round(fx.cur.brightness * 100) : 100
                            onMoved: {
                                const v = value / 100;
                                studio.updateCurrent(l => l.brightness = v);
                            }
                        }
                        Text {
                            text: (fx.cur && fx.cur.brightness !== undefined ? Math.round(fx.cur.brightness * 100) : 100) + "%"
                            color: Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                    }
                    RowLayout {
                        spacing: 10
                        Text {
                            text: "Blend"
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                        ModeTabs {
                            objectName: "blendTabs"
                            minTabWidth: 72
                            model: ["Cover", "Add", "Lighten"]
                            currentIndex: fx.cur ? Math.max(0, ["cover", "add", "lighten"].indexOf(fx.cur.blend || "cover")) : 0
                            onActivated: i => studio.updateCurrent(l => l.blend = ["cover", "add", "lighten"][i])
                        }
                        HelpButton {
                            size: 16
                            text: "Blend controls how this layer mixes with the layers below it. Cover replaces them on its LEDs, Add stacks the light, Lighten keeps whichever colour is brighter."
                        }
                    }
                    Text {
                        Layout.fillWidth: true
                        text: "Cover hides the layers below on this layer's LEDs; Add mixes its light on top; Lighten keeps the brighter colour."
                        color: Theme.textMuted
                        font.family: Theme.font
                        font.pixelSize: 11
                        wrapMode: Text.Wrap
                    }
                    SectionLabel {
                        caption: "Effect"
                    }
                    Flow {
                        Layout.fillWidth: true
                        Layout.preferredWidth: 1
                        spacing: 6
                        Repeater {
                            model: studio.effects
                            delegate: Rectangle {
                                required property var modelData
                                readonly property bool on: !!fx.cur && fx.cur.effect.type === modelData.type
                                width: el.implicitWidth + 18
                                height: 28
                                color: on ? Theme.accentTint : Theme.bg
                                border.color: on ? Theme.accent : Theme.borderSoft
                                Text {
                                    id: el
                                    anchors.centerIn: parent
                                    text: modelData.label
                                    color: on ? Theme.accent : Theme.text
                                    font.family: Theme.font
                                    font.pixelSize: Theme.small
                                }
                                TapHandler {
                                    onTapped: studio.updateCurrent(l => {
                                        l.effect = studio.newEffect(modelData.type);
                                        l.name = modelData.label;
                                    })
                                }
                            }
                        }
                    }
                    Text {
                        Layout.fillWidth: true
                        text: "Reactive and Ripple run in the keyboard itself: pick them on the device's LIGHTING tab."
                        color: Theme.textMuted
                        font.family: Theme.font
                        font.pixelSize: 11
                        wrapMode: Text.Wrap
                    }

                    // Colours
                    SectionLabel {
                        caption: "Colour"
                        visible: fx.info && fx.info.colours > 0
                    }
                    ModeTabs {
                        visible: fx.info && fx.info.random === true
                        minTabWidth: 96
                        model: ["Random", "Single", "Dual"]
                        currentIndex: fx.cur ? Math.min(2, (fx.cur.effect.colours || []).length) : 0
                        onActivated: index => studio.updateCurrent(l => {
                            const c = (l.effect.colours || []).slice(0, index);
                            while (c.length < index) {
                                c.push(c.length ? "0080ff" : "44d62c");
                            }
                            l.effect.colours = c;
                        })
                    }
                    Row {
                        spacing: 10
                        visible: fx.info && fx.info.colours > 0
                        Repeater {
                            model: {
                                const l = fx.cur;
                                if (!l) {
                                    return [];
                                }
                                return l.effect.type === "static" ? [l.effect.colour] : (l.effect.colours || []);
                            }
                            delegate: Rectangle {
                                required property string modelData
                                required property int index
                                readonly property bool sel: Math.min(studio.editingColour, fx.slots - 1) === index
                                width: 36
                                height: 36
                                radius: 18
                                color: "#" + modelData
                                border.color: sel ? Theme.accent : Theme.border
                                border.width: sel ? 3 : 1
                                TapHandler {
                                    onTapped: studio.editingColour = index
                                }
                            }
                        }
                        Text {
                            visible: fx.slots === 0
                            height: 36
                            verticalAlignment: Text.AlignVCenter
                            text: "Random colours"
                            color: Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                    }
                    ColourPanel {
                        objectName: "colourPanel"
                        Layout.fillWidth: true
                        visible: fx.slots > 0
                        colour: studio.editedColour() || "44d62c"
                        onEdited: hex => studio.setColour(hex)
                    }

                    // Duration
                    SectionLabel {
                        caption: "Duration"
                        visible: !!fx.cur && ["static", "audio_meter", "ambient", "battery"].indexOf(fx.cur.effect.type) < 0
                    }
                    ModeTabs {
                        visible: !!fx.cur && ["static", "audio_meter", "ambient", "battery"].indexOf(fx.cur.effect.type) < 0
                        minTabWidth: 96
                        model: ["Short", "Medium", "Long"]
                        currentIndex: fx.cur ? ["short", "medium", "long"].indexOf(fx.cur.effect.duration || "medium") : 1
                        onActivated: index => studio.updateCurrent(l => l.effect.duration = ["short", "medium", "long"][index])
                    }

                    // Direction
                    SectionLabel {
                        caption: "Direction"
                        visible: fx.info && fx.info.directions !== undefined
                    }
                    Column {
                        spacing: 6
                        visible: fx.info && fx.info.directions !== undefined
                        Repeater {
                            model: fx.info && fx.info.directions ? fx.info.directions : []
                            delegate: Rectangle {
                                required property string modelData
                                required property int index
                                readonly property var lay: fx.cur
                                readonly property bool on: !!lay && (lay.effect.type === "tidal" ? (lay.effect.inward === (index === 1)) : studio.dirKeys[lay.effect.type][index] === lay.effect.direction)
                                width: 240
                                height: 30
                                color: on ? Theme.accentTint : Theme.bg
                                border.color: on ? Theme.accent : Theme.borderSoft
                                Text {
                                    anchors.centerIn: parent
                                    text: modelData
                                    color: on ? Theme.accent : Theme.text
                                    font.family: Theme.font
                                    font.pixelSize: Theme.small
                                }
                                TapHandler {
                                    onTapped: studio.updateCurrent(l => {
                                        if (l.effect.type === "tidal") {
                                            l.effect.inward = index === 1;
                                        } else {
                                            l.effect.direction = studio.dirKeys[l.effect.type][index];
                                        }
                                    })
                                }
                            }
                        }
                    }

                    // Starlight density
                    SectionLabel {
                        caption: "Density"
                        visible: fx.info && fx.info.density === true
                    }
                    SynSlider {
                        Layout.fillWidth: true
                        visible: fx.info && fx.info.density === true
                        from: 0.02
                        to: 0.6
                        value: fx.cur && fx.cur.effect.density ? fx.cur.effect.density : 0.15
                        onMoved: {
                            const v = value;
                            studio.updateCurrent(l => l.effect.density = v);
                        }
                    }

                    // Audio Meter options
                    SectionLabel {
                        caption: "Sensitivity"
                        visible: fx.isMeter
                    }
                    SynSlider {
                        Layout.fillWidth: true
                        visible: fx.isMeter
                        from: 0.5
                        to: 3.0
                        value: fx.isMeter && fx.cur.effect.gain !== undefined ? fx.cur.effect.gain : 1.0
                        onMoved: {
                            const v = value;
                            studio.updateCurrent(l => l.effect.gain = v);
                        }
                    }
                    SectionLabel {
                        caption: "Colour boost"
                        visible: fx.isMeter
                    }
                    SynSlider {
                        Layout.fillWidth: true
                        visible: fx.isMeter
                        from: 0.4
                        to: 2.0
                        value: fx.isMeter && fx.cur.effect.boost !== undefined ? fx.cur.effect.boost : 1.0
                        onMoved: {
                            const v = value;
                            studio.updateCurrent(l => l.effect.boost = v);
                        }
                    }
                    SynCheckBox {
                        visible: fx.isMeter
                        text: "Flow (colours move across the bars)"
                        checked: fx.isMeter && !!fx.cur.effect.flow
                        onToggled: {
                            const v = checked;
                            studio.updateCurrent(l => l.effect.flow = v);
                        }
                    }

                    // Ambient options
                    SectionLabel {
                        caption: "Screen area"
                        visible: fx.isAmbient
                    }
                    AreaPicker {
                        visible: fx.isAmbient
                        Layout.fillWidth: true
                        Layout.preferredHeight: width * 9 / 16
                        backend: studio.backend
                        area: fx.isAmbient && fx.cur.effect.area ? fx.cur.effect.area : [0, 0, 1, 1]
                        onAreaEdited: a => studio.updateCurrent(l => l.effect.area = a)
                    }
                    SectionLabel {
                        caption: "Vividness"
                        visible: fx.isAmbient
                    }
                    SynSlider {
                        Layout.fillWidth: true
                        visible: fx.isAmbient
                        from: 0.6
                        to: 2.0
                        value: fx.isAmbient && fx.cur.effect.boost !== undefined ? fx.cur.effect.boost : 1.3
                        onMoved: {
                            const v = value;
                            studio.updateCurrent(l => l.effect.boost = v);
                        }
                    }
                }
                }
                Rectangle {
                    id: saveBar
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    height: 56
                    color: Theme.panel
                    Rectangle {
                        width: parent.width
                        height: 1
                        color: Theme.borderSoft
                    }
                    SynButton {
                        anchors.centerIn: parent
                        text: "Save"
                        primary: true
                        implicitWidth: 160
                        enabled: !studio.backend.readonly && studio.profile.name.trim() !== ""
                        onClicked: studio.saveProfile(true)
                    }
                }
            }
        }
    }

    SynapseImportDialog {
        id: synapseDlg
        objectName: "synapseImportDialog"
        anchors.fill: parent
        z: 50
        backend: studio.backend
        onImported: {
            studio.loadProfiles();
            Theme.profilesRev++;
        }
    }
}
