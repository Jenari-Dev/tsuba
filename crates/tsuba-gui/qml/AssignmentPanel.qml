import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtCore

// Left-docked assignment panel (Synapse-style): the button's current function, a green
// function-type row that opens the category list, and the editor for the chosen category.
// Choosing options only prepares a change (shown as "Selected: …"); SAVE writes it to the
// device's on-board profile through tsubad, CANCEL drops it.
Rectangle {
    id: root

    property var backend: null
    // Writes refused for this device (read-only daemon, or an untested model not yet allowed).
    property bool readOnly: backend ? backend.readonly : false
    property string pid: ""
    property bool isMouse: false
    property int keyLayer: 0
    property int profile: 1
    // The library profile being edited (its linked apps suggest shortcut presets).
    property string profileId: ""
    // How many library profiles the device has (the Switch profile hint needs two).
    property int profileCount: 2
    property int target: -1
    property string targetName: ""
    property var assignment: null
    property bool hypershift: false
    property var allAssignments: ({})   // position -> assignment on this layer (left-click safeguard)
    property var hypershiftPositions: []  // keys that activate Hypershift (locked on that layer)
    // Fn and the Hypershift keys (on the Hypershift layer) can't be remapped.
    readonly property bool locked: (!isMouse && target === 59) || (keyLayer === 1 && hypershiftPositions.indexOf(target) >= 0)
    signal assigned()
    signal closed()
    signal openMacros()
    property bool listOpen: false
    property var captureItem: null   // the keyboard-function capture box
    // Show the one-time "remapped keys can affect login" notice after the first remap. Off in tests.
    property bool remapNotice: true

    property string open: ""          // expanded category
    property var captured: null       // keyboard function [modifier, usage]
    property string result: ""
    property var macroLib: []          // library macros, loaded when the Macro category opens
    property int macroIdx: 0
    property int playback: 0           // once, times, while_held, toggle, sequence
    readonly property var playbackNames: [i18n("Play once"), i18n("Repeat a number of times"), i18n("Repeat while held"), i18n("Toggle continuous playback"), i18n("Play as a sequence (experimental)")]
    readonly property var playbackIds: ["once", "times", "while_held", "toggle", "sequence"]
    property int repeatCount: 2

    // The helper that talks to tsubad's Library interface (tests inject a fake `lib`).
    property alias lp: provider
    LibProvider {
        id: provider
    }

    // ---- the change waiting for SAVE: {id, label, run}; run returns false when it failed.
    property var staged: null
    readonly property string stagedId: staged ? staged.id : ""
    function stage(id, label, run) {
        staged = { id: id, label: label, run: run };
        result = "";
    }
    function clearStage() {
        staged = null;
    }

    // ---- remapped keys can affect the login screen
    Settings {
        id: warn
        location: StandardPaths.writableLocation(StandardPaths.ConfigLocation) + "/tsuba/app.ini"
        category: "warnings"
        property bool remapNoticeSeen: false
    }
    property bool noticeOpen: false

    function loadMacros() {
        const s = backend.macros();
        if (s === "") {
            return;
        }
        const j = JSON.parse(s);
        macroLib = j.library;
        if (macroLib.length === 0 && backend.env_var("TSUBA_DEMO") === "1") {   // headless UI check
            macroLib = [{ guid: "d1", name: "Demo: copy line" }, { guid: "d2", name: "Demo: sign-off" }, { guid: "d3", name: "Demo: open terminal" }];
        }
        const names = {};
        for (const p in j.devices) {
            for (const m of j.devices[p].macros) {
                names[m.id] = m.name;
            }
        }
        KeyNames.macroNames = names;
        preselectMacro(j.devices[pid] ? j.devices[pid].macros : []);
        stageMacro();
    }
    // Pre-pick the macro and playback option already bound to the selected key (if any).
    function preselectMacro(onBoard) {
        macroIdx = 0;
        playback = 0;
        const a = assignment;
        if (!a || [3, 4, 5, 15].indexOf(a.function) < 0 || !a.data || a.data.length < 2) {
            return;
        }
        const id = a.data[0] * 256 + a.data[1];
        const dm = onBoard.find(m => m.id === id);
        const i = dm ? macroLib.findIndex(m => m.guid === dm.guid) : -1;
        if (i >= 0) {
            macroIdx = i;
        }
        // Function 3 plays N times (1 = once); 4 repeats while held; 5 toggles; 15 is a sequence.
        if (a.function === 4) {
            playback = 2;
        } else if (a.function === 5) {
            playback = 3;
        } else if (a.function === 15) {
            playback = 4;
        } else if (a.data.length > 2 && a.data[2] > 1) {
            playback = 1;
            repeatCount = a.data[2];
        }
    }
    // The macro dropdown's current choice is what SAVE writes.
    function stageMacro() {
        const m = macroLib[macroIdx];
        if (open !== "macro" || !m) {
            return;
        }
        stage("macro", i18n("Macro: %1", m.name), () => assignMacro());
    }
    onMacroIdxChanged: stageMacro()
    onPlaybackChanged: stageMacro()
    onRepeatCountChanged: stageMacro()

    // Software actions: the key sends a shortcut-slot key combination that tsubad acts on.
    function assignAction(action) {
        const r = backend.assign_action(pid, JSON.stringify({ profile: profile, key: target, layer: keyLayer, action: action }));
        result = r === "" ? "Failed: " + backend.status : "Saved: " + r;
        if (r !== "") {
            afterSave(true);
            return true;
        }
        return false;
    }
    readonly property var otherDevices: JSON.parse(backend ? (backend.devices || "[]") : "[]").filter(d => d.pid !== pid)
    property int interTarget: 0
    readonly property var interOps: [
        [i18n("Sensitivity stage up"), { op: "dpi_stage_up" }], [i18n("Sensitivity stage down"), { op: "dpi_stage_down" }],
        [i18n("Sensitivity clutch (hold)"), null], [i18n("Next profile"), { op: "profile_next" }], [i18n("Previous profile"), { op: "profile_previous" }],
        [i18n("Brightness up"), { op: "brightness_up" }], [i18n("Brightness down"), { op: "brightness_down" }], [i18n("Lighting on / off"), { op: "lighting_toggle" }]
    ]

    // A library macro for `events`, reusing an identical one; "" on failure.
    function ensureMacro(name, events) {
        const lib = JSON.parse(backend.macros() || "{\"library\": []}").library;
        const same = lib.find(m => JSON.stringify(m.events) === JSON.stringify(events));
        if (same) {
            return same.guid;
        }
        return backend.save_macro(JSON.stringify({ guid: "", name: name.replace(/\n/g, " "), events: events }));
    }
    // Bind the key to a macro made from `events`, played once. Returns whether it worked.
    function assignEventsMacro(name, events) {
        const guid = ensureMacro(name, events);
        if (guid === "") {
            result = "Failed: " + backend.status;
            return false;
        }
        const r = backend.assign_macro(pid, JSON.stringify({ profile: profile, key: target, layer: keyLayer, guid: guid, playback: "once", count: 1 }));
        result = r === "" ? "Failed: " + backend.status : "Saved: " + name;
        if (r !== "") {
            afterSave(true);
            return true;
        }
        return false;
    }
    function assignComboMacro(name, events) {
        return assignEventsMacro(name, events);
    }
    function assignMacro() {
        const m = macroLib[macroIdx];
        if (!m) {
            return false;
        }
        const r = backend.assign_macro(pid, JSON.stringify({ profile: profile, key: target, layer: keyLayer, guid: m.guid, playback: playbackIds[playback], count: repeatCount }));
        result = r === "" ? "Failed: " + backend.status : "Saved: " + r;
        console.log("assign macro", pid, target, keyLayer, m.name, "->", result);
        if (r !== "") {
            afterSave(true);
            return true;
        }
        return false;
    }

    // fn = the assignment function a category writes; alt = other functions it also covers.
    // The order is Synapse's, with Tsuba's extras after Multimedia.
    readonly property var categories: [
        { id: "default", name: i18n("Default"), fn: -1 },
        { id: "keyboard", name: i18n("Keyboard function"), fn: 2, alt: [13] },
        { id: "mouse", name: i18n("Mouse function"), fn: 1, alt: [11, 14] },
        { id: "sensitivity", name: i18n("Sensitivity"), fn: 6 },
        { id: "macro", name: i18n("Macro"), fn: 3, alt: [4, 5, 15] },
        { id: "interdevice", name: i18n("Inter-device"), fn: -5 },
        { id: "profile", name: i18n("Switch profile"), fn: 7 },
        { id: "lighting", name: i18n("Lighting effect"), fn: -6 },
        { id: "hypershift", name: i18n("Shift layer"), fn: 12 },
        { id: "launch", name: i18n("Launch program / website"), fn: -4 },
        { id: "media", name: i18n("Multimedia"), fn: 10 },
        { id: "audio", name: i18n("Audio"), fn: -7 },
        { id: "shortcuts", name: i18n("Shortcuts"), fn: -3 },
        { id: "text", name: i18n("Text function"), fn: -2 },
        { id: "system", name: i18n("System (power / sleep)"), fn: 9 },
        { id: "scroll", name: i18n("Scroll wheel"), fn: 18, mouseOnly: true },
        { id: "disable", name: i18n("Disable"), fn: 0 }
    ]
    // Repeat-while-held for keyboard functions.
    property bool turbo: false
    property int turboRate: 7
    function turboData() {
        const ms = Math.round(1000 / Math.max(1, turboRate));
        return [ms >> 8, ms & 255];
    }

    function defaultFor(pos) {
        return KeyNames.defaultFor(isMouse, pos, keyLayer);
    }
    function save(fn, data) {
        // Left click must stay on at least one mouse button.
        if (isMouse && !(fn === 1 && data[0] === 1)) {
            const cur = allAssignments[target];
            const isLeft = a => a && a.function === 1 && a.data[0] === 1;
            if (isLeft(cur) && !Object.keys(allAssignments).some(k => +k !== target && isLeft(allAssignments[k]))) {
                result = "Failed: left click has to stay on at least one button";
                return false;
            }
        }
        const r = backend.set_assignment(pid, JSON.stringify({ profile: profile, key: target, layer: keyLayer, function: fn, data: data }));
        result = r === "" ? "Failed: " + backend.status : "Saved: " + r;
        console.log("assignment", pid, target, keyLayer, fn, JSON.stringify(data), "->", result);
        if (r !== "") {
            const d = defaultFor(target);
            afterSave(!d || d.function !== fn || JSON.stringify(d.data) !== JSON.stringify(data));
            return true;
        }
        return false;
    }
    // A change was written. A first remap shows the one-time login notice.
    function afterSave(remapped) {
        assigned();
        if (remapped && remapNotice && !warn.remapNoticeSeen) {
            noticeOpen = true;
        }
    }

    // The category the current assignment belongs to (shown as the type row).
    readonly property var currentCategory: {
        const a = assignment;
        return a ? categories.find(c => c.fn === a.function || (c.alt || []).indexOf(a.function) >= 0) || null : null;
    }
    readonly property var shownCategory: categories.find(c => c.id === open) || currentCategory
    // Names of the apps the edited profile is linked to (for suggested shortcut areas).
    function linkedAppNames() {
        if (profileId === "" || !backend || typeof backend.app_links !== "function") {
            return [];
        }
        const s = backend.app_links();
        if (s === "") {
            return [];
        }
        return JSON.parse(s).links.filter(l => l.device_profiles && l.device_profiles[pid] === profileId).map(l => l.name || l.matches);
    }

    function openCategory(id) {
        open = id;
        listOpen = false;
        result = "";
        captured = null;
        staged = null;
        if (id === "keyboard" && captureItem) {
            captureItem.forceActiveFocus();
        }
        if (id === "macro") {
            loadMacros();
        }
        if (id === "shortcuts") {
            shortcutCat.linkedApps = linkedAppNames();
        }
    }
    // SAVE writes what was selected; with nothing selected it just closes.
    function commit() {
        if (staged !== null && !readOnly) {
            if (staged.run() === false) {
                return;
            }
            staged = null;
        }
        closed();
    }
    function cancel() {
        staged = null;
        closed();
    }
    // A key was picked or pressed for the keyboard function.
    function stageKey(d) {
        captured = d;
        stage("key", KeyNames.legend({ function: 2, data: d }) + (turbo ? i18n(" (turbo %1/s)", turboRate) : ""), () => turbo ? save(13, d.concat(turboData())) : save(2, d));
    }

    onTargetChanged: {
        open = "";
        captured = null;
        result = "";
        listOpen = false;
        staged = null;
        // Show the options of the function the key currently has (after the assignment binding updates).
        Qt.callLater(() => {
            const force = root.backend && typeof root.backend.env_var === "function" ? root.backend.env_var("TSUBA_CATEGORY") : "";   // headless UI check
            if (force !== "" && root.target >= 0) {
                root.openCategory(force);
            } else if (root.open === "" && root.currentCategory) {
                root.openCategory(root.currentCategory.id);
            }
        });
    }

    color: Theme.panel
    border.color: Theme.borderSoft

    // CANCEL / SAVE footer
    Rectangle {
        id: footer
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: 64
        color: Theme.panel
        Rectangle {
            width: parent.width
            height: 1
            color: Theme.borderSoft
        }
        Row {
            anchors.centerIn: parent
            spacing: 12
            Rectangle {
                objectName: "cancelButton"
                width: 130
                height: 34
                radius: 3
                color: cancelHover.hovered ? "#707070" : "#5d5d5d"
                Text {
                    anchors.centerIn: parent
                    text: i18n("CANCEL")
                    color: Theme.textBright
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                }
                HoverHandler {
                    id: cancelHover
                    cursorShape: Qt.PointingHandCursor
                }
                TapHandler {
                    onTapped: root.cancel()
                }
            }
            Rectangle {
                objectName: "saveButton"
                width: 130
                height: 34
                radius: 3
                opacity: root.readOnly ? 0.4 : 1
                color: saveHover.hovered ? Theme.accentHover : Theme.accent
                Text {
                    anchors.centerIn: parent
                    text: i18n("SAVE")
                    color: Theme.accentText
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                }
                HoverHandler {
                    id: saveHover
                    cursorShape: Qt.PointingHandCursor
                }
                TapHandler {
                    onTapped: root.commit()
                }
            }
        }
    }

    QQC2.ScrollView {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: footer.top
        contentWidth: availableWidth

        ColumnLayout {
            width: root.width - 36
            x: 18
            spacing: 6

            // Header: the function the button has now (Synapse shows it there) + close
            Item {
                Layout.fillWidth: true
                Layout.preferredHeight: 46
                Column {
                    anchors.centerIn: parent
                    width: parent.width - 60
                    Text {
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                        text: root.targetName === "" ? i18n("Select a key or button") : (root.assignment ? KeyNames.describe(root.assignment) : root.targetName)
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.fontSize
                        elide: Text.ElideRight
                    }
                    Text {
                        visible: root.targetName !== "" && !!root.assignment
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                        text: root.targetName
                        color: Theme.textMuted
                        font.family: Theme.font
                        font.pixelSize: 11
                        elide: Text.ElideRight
                    }
                }
                Text {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: "✕"
                    color: xh.hovered ? Theme.textBright : Theme.text
                    font.pixelSize: 16
                    HoverHandler {
                        id: xh
                        cursorShape: Qt.PointingHandCursor
                    }
                    TapHandler {
                        onTapped: root.cancel()
                    }
                }
            }
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 1
                color: Theme.borderSoft
            }
            // Function type row: green, opens the category list.
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 38
                visible: root.targetName !== "" && !root.locked
                color: typeHover.hovered ? "#2d2d2d" : "transparent"
                Text {
                    anchors.left: parent.left
                    anchors.leftMargin: 4
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.shownCategory ? root.shownCategory.name.toUpperCase() : i18n("SELECT A FUNCTION")
                    color: Theme.accent
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    font.letterSpacing: 0.5
                }
                Text {
                    anchors.right: parent.right
                    anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.listOpen ? "▴" : "▾"
                    color: Theme.text
                    font.pixelSize: 11
                }
                HoverHandler {
                    id: typeHover
                    cursorShape: Qt.PointingHandCursor
                }
                TapHandler {
                    onTapped: root.listOpen = !root.listOpen
                }
            }
            Text {
                visible: root.locked && root.targetName !== ""
                Layout.fillWidth: true
                text: root.target === 59 && !root.isMouse ? i18n("Fn is the keyboard's shift-layer key and can't be remapped (the firmware reserves it).") : i18n("This key turns the shift layer on, so it can't have a shift-layer assignment.")
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: Theme.small
                wrapMode: Text.Wrap
            }

            // The category list (Synapse's order); the current function's category is green.
            Repeater {
                model: root.listOpen && !root.locked ? root.categories : []
                delegate: Rectangle {
                    required property var modelData
                    readonly property bool current: !!root.assignment && (root.assignment.function === modelData.fn || (modelData.alt || []).indexOf(root.assignment.function) >= 0)
                    objectName: "category_" + modelData.id
                    visible: !modelData.mouseOnly || root.isMouse
                    Layout.fillWidth: true
                    Layout.preferredHeight: visible ? 32 : 0
                    color: ch.hovered ? "#2d2d2d" : "transparent"
                    opacity: root.target < 0 ? 0.4 : 1
                    Text {
                        anchors.left: parent.left
                        anchors.leftMargin: 4
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData.name.toUpperCase()
                        color: root.open === modelData.id || (root.open === "" && parent.current) ? Theme.accent : Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        font.letterSpacing: 0.5
                    }
                    HoverHandler {
                        id: ch
                        cursorShape: root.target >= 0 ? Qt.PointingHandCursor : Qt.ArrowCursor
                    }
                    TapHandler {
                        enabled: root.target >= 0
                        onTapped: root.openCategory(modelData.id)
                    }
                }
            }

            // ---- editors: one per category, only the open one shows
            ColumnLayout {
                id: editors
                visible: !root.listOpen && !root.locked && root.open !== ""
                Layout.fillWidth: true
                Layout.leftMargin: 4
                spacing: 6

                // Default
                ColumnLayout {
                    visible: root.open === "default"
                    Layout.fillWidth: true
                    spacing: 6
                    Text {
                        Layout.fillWidth: true
                        readonly property var d: root.defaultFor(root.target)
                        text: d ? i18n("Default: ") + KeyNames.describe({ function: d.function, data: d.data, label: "" }) : i18n("Factory default for this button isn't known yet.")
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        wrapMode: Text.Wrap
                    }
                    Item {
                        implicitWidth: restoreBtn.implicitWidth
                        implicitHeight: restoreBtn.implicitHeight
                        SynButton {
                            id: restoreBtn
                            objectName: "restoreDefault"
                            anchors.fill: parent
                            text: i18n("Reset to default")
                            primary: true
                            enabled: !root.readOnly && root.defaultFor(root.target) !== null
                            onClicked: {
                                const d = root.defaultFor(root.target);
                                root.stage("default", i18n("Default: ") + KeyNames.describe({ function: d.function, data: d.data, label: "" }), () => root.save(d.function, d.data));
                            }
                        }
                        HoverHandler { id: restoreHover }
                        QQC2.ToolTip {
                            visible: restoreHover.hovered && root.defaultFor(root.target) === null
                            text: KeyNames.defaultUnknownTip
                        }
                    }
                }

                // Keyboard function: press the key to assign, or pick from the grouped list.
                ColumnLayout {
                    visible: root.open === "keyboard"
                    Layout.fillWidth: true
                    spacing: 6
                    Rectangle {
                        id: capture
                        objectName: "keyCapture"
                        Layout.fillWidth: true
                        Layout.preferredHeight: 44
                        color: Theme.card
                        radius: 2
                        border.color: activeFocus ? Theme.accent : Theme.border
                        focus: root.open === "keyboard"
                        Component.onCompleted: root.captureItem = capture
                        Text {
                            anchors.centerIn: parent
                            text: root.captured ? KeyNames.legend({ function: 2, data: root.captured }) : (capture.activeFocus ? i18n("Press a key or combination…") : i18n("Click here, then press a key"))
                            color: root.captured ? Theme.accent : Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: Theme.fontSize
                        }
                        TapHandler {
                            onTapped: capture.forceActiveFocus()
                        }
                        Keys.onPressed: ev => {
                            const d = KeyNames.fromKeyEvent(ev);
                            if (d) {
                                root.stageKey(d);
                            }
                            ev.accepted = true;
                        }
                    }
                    SectionHeading { text: i18n("OR PICK A KEY") }
                    KeyPicker {
                        objectName: "keyboardPicker"
                        Layout.fillWidth: true
                        onPicked: (data, usage) => root.stageKey(data)
                    }
                    RowLayout {
                        spacing: 8
                        SynCheckBox {
                            text: i18n("Turbo")
                            checked: root.turbo
                            onToggled: {
                                root.turbo = checked;
                                if (root.captured) {
                                    root.stageKey(root.captured);
                                }
                            }
                        }
                        QQC2.SpinBox {
                            enabled: root.turbo
                            from: 1
                            to: 30
                            value: root.turboRate
                            onValueModified: {
                                root.turboRate = value;
                                if (root.captured) {
                                    root.stageKey(root.captured);
                                }
                            }
                        }
                        Text {
                            text: i18n("per second")
                            color: Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                    }
                }

                // Mouse function
                MouseEditor {
                    id: mouseEd
                    objectName: "mouseEditor"
                    visible: root.open === "mouse"
                    Layout.fillWidth: true
                    panel: root
                }

                // Sensitivity
                ColumnLayout {
                    visible: root.open === "sensitivity"
                    Layout.fillWidth: true
                    spacing: 6
                    Repeater {
                        model: [[i18n("Sensitivity stage up"), 1], [i18n("Sensitivity stage down"), 2], [i18n("Cycle up sensitivity stages"), 6], [i18n("Cycle down sensitivity stages"), 7]]
                        delegate: OptionRow {
                            required property var modelData
                            label: modelData[0]
                            on: root.assignment && root.assignment.function === 6 && root.assignment.data[0] === modelData[1]
                            chosen: root.stagedId === "dpi:" + modelData[1]
                            onPicked: root.stage("dpi:" + modelData[1], modelData[0], () => root.save(6, [modelData[1]]))
                        }
                    }
                    RowLayout {
                        spacing: 8
                        Text {
                            text: i18n("Clutch")
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                        QQC2.SpinBox {
                            id: clutchDpi
                            from: 100
                            to: 30000
                            stepSize: 50
                            editable: true
                            value: root.assignment && root.assignment.function === 6 && root.assignment.data[0] === 5 ? (root.assignment.data[1] << 8) | root.assignment.data[2] : 400
                        }
                        SynButton {
                            text: i18n("Select")
                            implicitWidth: 72
                            onClicked: root.stage("dpi:clutch", i18n("Sensitivity clutch %1 DPI", clutchDpi.value), () => root.save(6, [5, clutchDpi.value >> 8, clutchDpi.value & 255, clutchDpi.value >> 8, clutchDpi.value & 255]))
                        }
                    }
                    RowLayout {
                        spacing: 8
                        Text {
                            text: i18n("Jump to stage")
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                        QQC2.SpinBox {
                            id: jumpStage
                            from: 1
                            to: 5
                            value: root.assignment && root.assignment.function === 6 && root.assignment.data[0] === 3 ? root.assignment.data[1] : 1
                        }
                        SynButton {
                            text: i18n("Select")
                            implicitWidth: 72
                            onClicked: root.stage("dpi:jump", i18n("Jump to stage %1", jumpStage.value), () => root.save(6, [3, jumpStage.value]))
                        }
                    }
                }

                // Macro: pick from the library + playback option
                ColumnLayout {
                    visible: root.open === "macro"
                    Layout.fillWidth: true
                    spacing: 8
                    Text {
                        visible: root.macroLib.length === 0
                        Layout.fillWidth: true
                        text: i18n("No macros yet. Record one in the Macros module (Home → Macros).")
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        wrapMode: Text.Wrap
                    }
                    Text {
                        visible: root.macroLib.length > 0
                        text: i18n("Macro")
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                    }
                    SynDropdown {
                        id: macroPick
                        objectName: "macroPick"
                        visible: root.macroLib.length > 0
                        Layout.fillWidth: true
                        model: root.macroLib.map(m => m.name)
                        currentIndex: root.macroIdx
                        onActivated: i => root.macroIdx = i
                    }
                    Text {
                        visible: root.macroLib.length > 0
                        text: i18n("Playback option")
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                    }
                    SynDropdown {
                        id: playbackPick
                        objectName: "playbackPick"
                        visible: root.macroLib.length > 0
                        Layout.fillWidth: true
                        model: root.playbackNames
                        currentIndex: root.playback
                        onActivated: i => root.playback = i
                    }
                    RowLayout {
                        visible: root.macroLib.length > 0 && root.playback === 1
                        spacing: 8
                        Text {
                            text: i18n("Times")
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                        QQC2.SpinBox {
                            from: 2
                            to: 255
                            editable: true
                            value: root.repeatCount
                            onValueModified: root.repeatCount = value
                        }
                    }
                    Text {
                        objectName: "manageMacros"
                        text: i18n("Manage macros")
                        color: mmh.hovered ? Theme.accentHover : Theme.accent
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        font.underline: true
                        HoverHandler {
                            id: mmh
                            cursorShape: Qt.PointingHandCursor
                        }
                        TapHandler {
                            onTapped: root.openMacros()
                        }
                    }
                }

                // Multimedia (HID consumer usages)
                ColumnLayout {
                    visible: root.open === "media"
                    Layout.fillWidth: true
                    spacing: 6
                    Repeater {
                        model: [[i18n("Play / Pause"), 0xcd], [i18n("Next track"), 0xb5], [i18n("Previous track"), 0xb6], [i18n("Stop"), 0xb7], [i18n("Mute"), 0xe2], [i18n("Volume up"), 0xe9], [i18n("Volume down"), 0xea]]
                        delegate: OptionRow {
                            required property var modelData
                            label: modelData[0]
                            on: root.assignment && root.assignment.function === 10 && root.assignment.data[1] === modelData[1]
                            chosen: root.stagedId === "media:" + modelData[1]
                            onPicked: root.stage("media:" + modelData[1], modelData[0], () => root.save(10, [0, modelData[1]]))
                        }
                    }
                }

                // Switch profile
                ColumnLayout {
                    visible: root.open === "profile"
                    Layout.fillWidth: true
                    spacing: 6
                    Text {
                        objectName: "profileHint"
                        visible: root.profileCount < 2
                        Layout.fillWidth: true
                        text: i18n("There is only one profile. Add another in the profile menu (•••) so this key has something to switch to.")
                        color: Theme.warn
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        wrapMode: Text.Wrap
                    }
                    Repeater {
                        model: [[i18n("Next profile"), 1], [i18n("Previous profile"), 2], [i18n("Cycle forward"), 4], [i18n("Cycle backward"), 5]]
                        delegate: OptionRow {
                            required property var modelData
                            label: modelData[0]
                            on: !!root.assignment && root.assignment.function === 7 && root.assignment.data[0] === modelData[1]
                            chosen: root.stagedId === "profile:" + modelData[1]
                            onPicked: root.stage("profile:" + modelData[1], modelData[0], () => root.save(7, [modelData[1]]))
                        }
                    }
                    RowLayout {
                        spacing: 8
                        Text {
                            text: i18n("Switch to on-board slot")
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                        QQC2.SpinBox {
                            id: profileTarget
                            from: 1
                            to: 5
                            value: root.assignment && root.assignment.function === 7 && root.assignment.data[0] === 3 ? root.assignment.data[1] : 1
                        }
                        SynButton {
                            text: i18n("Select")
                            implicitWidth: 72
                            onClicked: root.stage("profile:slot", i18n("Switch to slot %1", profileTarget.value), () => root.save(7, [3, profileTarget.value]))
                        }
                    }
                }

                // Hypershift
                ColumnLayout {
                    visible: root.open === "hypershift"
                    Layout.fillWidth: true
                    spacing: 6
                    Text {
                        Layout.fillWidth: true
                        text: i18n("While this is held, every key and button uses its shift-layer assignment (edit those on the SHIFT LAYER tab). Razer's own software calls this HyperShift.")
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        wrapMode: Text.Wrap
                    }
                    SynButton {
                        text: i18n("Make this the shift-layer key")
                        primary: true
                        enabled: !root.readOnly && root.keyLayer === 0
                        onClicked: root.stage("shift", i18n("Shift-layer key"), () => root.save(12, [1]))
                    }
                }

                // System keys
                ColumnLayout {
                    visible: root.open === "system"
                    Layout.fillWidth: true
                    spacing: 6
                    Repeater {
                        model: [[i18n("Power"), 129], [i18n("Sleep"), 130], [i18n("Wake"), 131]]
                        delegate: OptionRow {
                            required property var modelData
                            label: modelData[0]
                            on: !!root.assignment && root.assignment.function === 9 && root.assignment.data[0] === modelData[1]
                            chosen: root.stagedId === "system:" + modelData[1]
                            onPicked: root.stage("system:" + modelData[1], modelData[0], () => root.save(9, [modelData[1]]))
                        }
                    }
                }

                // Scroll wheel (mice)
                ColumnLayout {
                    visible: root.open === "scroll"
                    Layout.fillWidth: true
                    spacing: 6
                    Repeater {
                        model: [[i18n("Tactile / free-spin toggle"), 1], [i18n("Scroll stage up"), 2], [i18n("Scroll stage down"), 3], [i18n("Cycle scroll stages up"), 4], [i18n("Cycle scroll stages down"), 5]]
                        delegate: OptionRow {
                            required property var modelData
                            label: modelData[0]
                            on: !!root.assignment && root.assignment.function === 18 && root.assignment.data[0] === modelData[1]
                            chosen: root.stagedId === "scroll:" + modelData[1]
                            onPicked: root.stage("scroll:" + modelData[1], modelData[0], () => root.save(18, [modelData[1]]))
                        }
                    }
                }

                // Shortcuts (key combinations)
                ShortcutCatalogue {
                    id: shortcutCat
                    objectName: "shortcutCatalogue"
                    visible: root.open === "shortcuts"
                    Layout.fillWidth: true
                    panel: root
                }

                // Launch a program or a website (needs tsubad running).
                LaunchEditor {
                    id: launchEd
                    objectName: "launchEditor"
                    visible: root.open === "launch"
                    Layout.fillWidth: true
                    panel: root
                }

                // Inter-device: control another Razer device from this key/button.
                ColumnLayout {
                    visible: root.open === "interdevice"
                    Layout.fillWidth: true
                    spacing: 8
                    Text {
                        visible: root.otherDevices.length === 0
                        text: i18n("Connect another Razer device to use this.")
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                    }
                    SynDropdown {
                        visible: root.otherDevices.length > 0
                        Layout.fillWidth: true
                        model: root.otherDevices.map(d => d.name.replace(" (wired)", ""))
                        currentIndex: root.interTarget
                        onActivated: i => root.interTarget = i
                    }
                    Repeater {
                        model: root.otherDevices.length > 0 ? root.interOps : []
                        delegate: OptionRow {
                            required property var modelData
                            label: modelData[0]
                            chosen: root.stagedId === "inter:" + modelData[0]
                            onPicked: {
                                const op = modelData[1] || { op: "dpi_clutch", dpi: interClutch.value };
                                root.stage("inter:" + modelData[0], modelData[0], () => root.assignAction({ type: "device", target: root.otherDevices[root.interTarget].pid, op: op }));
                            }
                        }
                    }
                    RowLayout {
                        visible: root.otherDevices.length > 0
                        spacing: 8
                        Text {
                            text: i18n("Clutch DPI")
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                        QQC2.SpinBox {
                            id: interClutch
                            from: 100
                            to: 30000
                            stepSize: 50
                            editable: true
                            value: 400
                        }
                    }
                }

                // Lighting (Chroma functions): this device, another one, or all of them.
                ColumnLayout {
                    id: lightingEd
                    visible: root.open === "lighting"
                    Layout.fillWidth: true
                    spacing: 8
                    property var studioNames: []
                    readonly property var targets: [{ pid: root.pid, name: i18n("This device") }, { pid: "all", name: i18n("All devices") }].concat(root.otherDevices.map(d => ({ pid: d.pid, name: d.name.replace(" (wired)", "") })))
                    property int targetIdx: 0
                    readonly property bool all: targets[Math.min(targetIdx, targets.length - 1)].pid === "all"
                    // Brightness steps reach every device; the rest is per device.
                    readonly property var ops: all
                        ? [[i18n("Brightness up"), { op: "brightness_all_up" }], [i18n("Brightness down"), { op: "brightness_all_down" }]]
                        : [
                            [i18n("Next effect"), { op: "effect_next" }], [i18n("Previous effect"), { op: "effect_previous" }],
                            [i18n("Brightness up"), { op: "brightness_up" }], [i18n("Brightness down"), { op: "brightness_down" }],
                            [i18n("Lighting on / off"), { op: "lighting_toggle" }],
                            [i18n("Spectrum cycling"), { op: "effect", effect: { type: "spectrum" } }],
                            [i18n("Wave"), { op: "effect", effect: { type: "wave", direction: 1, speed: 40 } }],
                            [i18n("Breathing (random)"), { op: "effect", effect: { type: "breathing", colours: [] } }],
                            [i18n("Starlight"), { op: "effect", effect: { type: "starlight", speed: 2, colours: [] } }],
                            [i18n("Lighting off"), { op: "effect", effect: { type: "off" } }]
                        ]
                    function loadStudio() {
                        const s = root.backend && typeof root.backend.chroma_profiles === "function" ? root.backend.chroma_profiles() : "";
                        studioNames = s === "" ? [] : JSON.parse(s).profiles.map(p => p.name);
                    }
                    onVisibleChanged: if (visible) loadStudio()
                    SynDropdown {
                        id: lightTarget
                        objectName: "lightTarget"
                        Layout.fillWidth: true
                        model: lightingEd.targets.map(t => t.name)
                        currentIndex: lightingEd.targetIdx
                        onActivated: i => lightingEd.targetIdx = i
                    }
                    Repeater {
                        model: lightingEd.ops
                        delegate: OptionRow {
                            required property var modelData
                            objectName: "light_" + modelData[1].op
                            label: modelData[0]
                            chosen: root.stagedId === "light:" + lightingEd.targetIdx + ":" + modelData[0]
                            onPicked: {
                                const tgt = lightingEd.targets[lightingEd.targetIdx].pid;
                                root.stage("light:" + lightingEd.targetIdx + ":" + modelData[0], modelData[0], () => root.assignAction({ type: "device", target: tgt, op: modelData[1] }));
                            }
                        }
                    }
                    SectionHeading {
                        text: i18n("LIGHTING PROFILES (LIGHTING STUDIO)")
                    }
                    Repeater {
                        model: [[i18n("Next lighting profile"), { op: "studio_next" }], [i18n("Previous lighting profile"), { op: "studio_previous" }]].concat(lightingEd.studioNames.map(n => [i18n("Apply “%1”", n), { op: "studio_profile", name: n }]))
                        delegate: OptionRow {
                            required property var modelData
                            objectName: "studio_" + (modelData[1].name || modelData[1].op)
                            label: modelData[0]
                            chosen: root.stagedId === "studio:" + modelData[0]
                            onPicked: root.stage("studio:" + modelData[0], modelData[0], () => root.assignAction({ type: "device", target: "all", op: modelData[1] }))
                        }
                    }
                }

                // Audio (desktop sound through PipeWire)
                AudioEditor {
                    id: audioEd
                    objectName: "audioEditor"
                    visible: root.open === "audio"
                    Layout.fillWidth: true
                    panel: root
                }

                // Text: typed by an on-board macro, so it works without Tsuba running.
                TextEditor {
                    id: textEd
                    objectName: "textEditor"
                    visible: root.open === "text"
                    Layout.fillWidth: true
                    panel: root
                }

                // Disable
                SynButton {
                    visible: root.open === "disable"
                    text: root.isMouse ? i18n("Disable this button") : i18n("Disable this key")
                    enabled: !root.readOnly
                    onClicked: root.stage("disable", i18n("Disable"), () => root.save(0, []))
                }
            }

            // What SAVE will do.
            Text {
                objectName: "pendingNote"
                Layout.fillWidth: true
                visible: root.staged !== null && !root.listOpen
                text: i18n("Selected: %1 — press SAVE to apply", root.staged ? root.staged.label : "")
                color: Theme.accent
                font.family: Theme.font
                font.pixelSize: Theme.small
                wrapMode: Text.Wrap
            }
            Text {
                Layout.fillWidth: true
                visible: root.result !== ""
                text: root.result
                color: root.result.startsWith("Saved") ? Theme.accent : Theme.warn
                font.family: Theme.font
                font.pixelSize: Theme.small
                wrapMode: Text.Wrap
            }
            Item {
                Layout.preferredHeight: 12
            }
        }
    }

    // The first remap: keys changed in the keyboard itself also apply at the login screen.
    Rectangle {
        id: notice
        objectName: "remapNotice"
        visible: root.noticeOpen
        anchors.fill: parent
        z: 20
        color: "#e61a1a1a"
        MouseArea {
            anchors.fill: parent
        }
        ColumnLayout {
            anchors.centerIn: parent
            width: parent.width - 48
            spacing: 14
            Text {
                Layout.fillWidth: true
                text: i18n("Remapped keys also apply at the login screen")
                color: Theme.textBright
                font.family: Theme.font
                font.pixelSize: Theme.fontSize
                font.bold: true
                wrapMode: Text.Wrap
            }
            Text {
                Layout.fillWidth: true
                text: i18n("Tsuba saves remaps in the keyboard or mouse itself, so they work everywhere, including when you type your password at the login screen, in the BIOS and on other computers. If you remap letters or numbers, keep that in mind before you log out.")
                color: Theme.text
                font.family: Theme.font
                font.pixelSize: Theme.small
                wrapMode: Text.Wrap
            }
            SynButton {
                objectName: "remapNoticeOk"
                text: i18n("OK, don't show again")
                primary: true
                Layout.alignment: Qt.AlignHCenter
                onClicked: {
                    warn.remapNoticeSeen = true;
                    root.noticeOpen = false;
                }
            }
        }
    }
}
