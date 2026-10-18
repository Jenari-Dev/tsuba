import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// Left-docked assignment panel (Synapse-style): the selected key/button's name, a green
// function-type row that opens the category list, and the editor for the chosen category.
// Saving writes the assignment to the device's on-board profile through tsubad.
Rectangle {
    id: root

    property var backend: null
    // Writes refused for this device (read-only daemon, or an untested model not yet allowed).
    property bool readOnly: backend ? backend.readonly : false
    property string pid: ""
    property bool isMouse: false
    property int keyLayer: 0
    property int profile: 1
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

    property string open: ""          // expanded category
    property var captured: null       // keyboard function [modifier, usage]
    property string result: ""
    property var macroLib: []          // library macros, loaded when the Macro category opens
    property int macroIdx: 0
    property int playback: 0           // once, times, while_held, toggle
    readonly property var playbackNames: ["Play once", "Repeat a number of times", "Repeat while held", "Toggle continuous playback"]
    property int repeatCount: 2

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
        // Function 3 plays N times (1 = once); 4 repeats while held; 5 toggles.
        if (a.function === 4) {
            playback = 2;
        } else if (a.function === 5) {
            playback = 3;
        } else if (a.data.length > 2 && a.data[2] > 1) {
            playback = 1;
            repeatCount = a.data[2];
        }
    }
    // Software actions: the key sends a dynamic Razer-key code that tsubad acts on.
    function assignAction(action) {
        const r = backend.assign_action(pid, JSON.stringify({ profile: profile, key: target, layer: keyLayer, action: action }));
        result = r === "" ? "Failed: " + backend.status : "Saved: " + r;
        if (r !== "") {
            assigned();
        }
    }
    readonly property var otherDevices: JSON.parse(backend ? (backend.devices || "[]") : "[]").filter(d => d.pid !== pid)
    property int interTarget: 0
    property int launchMode: 0   // 0 program, 1 website
    readonly property var interOps: [
        ["Sensitivity stage up", { op: "dpi_stage_up" }], ["Sensitivity stage down", { op: "dpi_stage_down" }],
        ["Sensitivity clutch (hold)", null], ["Next profile", { op: "profile_next" }], ["Previous profile", { op: "profile_previous" }],
        ["Brightness up", { op: "brightness_up" }], ["Brightness down", { op: "brightness_down" }], ["Lighting on / off", { op: "lighting_toggle" }]
    ]

    function assignText(text, delay) {
        const t = KeyNames.textToEvents(text, delay);
        if (t.events.length === 0) {
            result = "Failed: nothing typeable in that text";
            return;
        }
        const label = text.length > 24 ? text.slice(0, 24) + "…" : text;
        // Reuse an identical text macro instead of piling up copies.
        const lib = JSON.parse(backend.macros() || "{\"library\": []}").library;
        const same = lib.find(m => JSON.stringify(m.events) === JSON.stringify(t.events));
        const guid = same ? same.guid : backend.save_macro(JSON.stringify({ guid: "", name: "Text: " + label.replace(/\n/g, " "), events: t.events }));
        if (guid === "") {
            result = "Failed: " + backend.status;
            return;
        }
        const r = backend.assign_macro(pid, JSON.stringify({ profile: profile, key: target, layer: keyLayer, guid: guid, playback: "once", count: 1 }));
        result = r === "" ? "Failed: " + backend.status : "Saved: text" + (t.skipped ? " (skipped: " + t.skipped + ")" : "");
        if (r !== "") {
            assigned();
        }
    }
    function assignMacro() {
        const m = macroLib[macroIdx];
        if (!m) {
            return;
        }
        const r = backend.assign_macro(pid, JSON.stringify({ profile: profile, key: target, layer: keyLayer, guid: m.guid, playback: ["once", "times", "while_held", "toggle"][playback], count: repeatCount }));
        result = r === "" ? "Failed: " + backend.status : "Saved: " + r;
        console.log("assign macro", pid, target, keyLayer, m.name, "->", result);
        if (r !== "") {
            assigned();
        }
    }

    // fn = the assignment function a category writes; alt = other functions it also covers.
    readonly property var categories: [
        { id: "default", name: "Default", fn: -1 },
        { id: "keyboard", name: "Keyboard function", fn: 2, alt: [13] },
        { id: "mouse", name: "Mouse function", fn: 1, alt: [11, 14] },
        { id: "sensitivity", name: "Sensitivity", fn: 6 },
        { id: "macro", name: "Macro", fn: 3, alt: [4, 5, 15] },
        { id: "text", name: "Text function", fn: -2 },
        { id: "shortcuts", name: "Shortcuts", fn: -3 },
        { id: "launch", name: "Launch program / website", fn: -4 },
        { id: "interdevice", name: "Inter-device", fn: -5 },
        { id: "lighting", name: "Lighting effect", fn: -6 },
        { id: "audio", name: "Audio", fn: -7 },
        { id: "hypershift", name: "Shift layer", fn: 12 },
        { id: "profile", name: "Switch profile", fn: 7 },
        { id: "media", name: "Multimedia", fn: 10 },
        { id: "system", name: "System (power / sleep)", fn: 9 },
        { id: "scroll", name: "Scroll wheel", fn: 18, mouseOnly: true },
        { id: "disable", name: "Disable", fn: 0 }
    ]
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
                return;
            }
        }
        const r = backend.set_assignment(pid, JSON.stringify({ profile: profile, key: target, layer: keyLayer, function: fn, data: data }));
        result = r === "" ? "Failed: " + backend.status : "Saved: " + r;
        console.log("assignment", pid, target, keyLayer, fn, JSON.stringify(data), "->", result);
        if (r !== "") {
            assigned();
        }
    }

    // The category the current assignment belongs to (shown as the type row).
    readonly property var currentCategory: {
        const a = assignment;
        return a ? categories.find(c => c.fn === a.function || (c.alt || []).indexOf(a.function) >= 0) || null : null;
    }
    readonly property var shownCategory: categories.find(c => c.id === open) || currentCategory

    function openCategory(id) {
        open = id;
        listOpen = false;
        result = "";
        captured = null;
        if (id === "keyboard" && captureItem) {
            captureItem.forceActiveFocus();
        }
        if (id === "macro") {
            loadMacros();
        }
    }
    // SAVE: the editors that need a confirmation have their own button; keyboard function is
    // committed here, everything else is already written, so it just closes.
    function commit() {
        if (open === "keyboard" && captured !== null && !readOnly) {
            turbo ? save(13, captured.concat(turboData())) : save(2, captured);
        }
        if (open === "macro" && macroLib.length > 0 && !readOnly) {
            assignMacro();
        }
        closed();
    }

    onTargetChanged: {
        open = "";
        captured = null;
        result = "";
        listOpen = false;
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

    component Option: Rectangle {
        id: opt
        property string label: ""
        property bool on: false
        signal picked()
        Layout.fillWidth: true
        Layout.preferredHeight: 32
        color: on ? Theme.accentTint : (oh.hovered ? "#14ffffff" : Theme.bg)
        border.color: on ? Theme.accent : Theme.borderSoft
        Text {
            anchors.left: parent.left
            anchors.leftMargin: 10
            anchors.verticalCenter: parent.verticalCenter
            text: opt.label
            color: opt.on ? Theme.accent : Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.small
        }
        HoverHandler {
            id: oh
            cursorShape: Qt.PointingHandCursor
        }
        TapHandler {
            onTapped: opt.picked()
        }
    }

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
                width: 130
                height: 34
                radius: 3
                color: cancelHover.hovered ? "#707070" : "#5d5d5d"
                Text {
                    anchors.centerIn: parent
                    text: "CANCEL"
                    color: Theme.textBright
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                }
                HoverHandler {
                    id: cancelHover
                    cursorShape: Qt.PointingHandCursor
                }
                TapHandler {
                    onTapped: root.closed()
                }
            }
            Rectangle {
                width: 130
                height: 34
                radius: 3
                color: saveHover.hovered ? Theme.accentHover : Theme.accent
                Text {
                    anchors.centerIn: parent
                    text: "SAVE"
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

            // Header: button name + close
            Item {
                Layout.fillWidth: true
                Layout.preferredHeight: 46
                Text {
                    anchors.centerIn: parent
                    width: parent.width - 60
                    horizontalAlignment: Text.AlignHCenter
                    text: root.targetName === "" ? "Select a key or button" : root.targetName
                    color: Theme.text
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize
                    elide: Text.ElideRight
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
                        onTapped: root.closed()
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
                    text: root.shownCategory ? root.shownCategory.name.toUpperCase() : "SELECT A FUNCTION"
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
                visible: root.targetName !== "" && !root.listOpen && !root.locked
                Layout.fillWidth: true
                Layout.leftMargin: 4
                text: "Current: " + KeyNames.describe(root.assignment)
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: Theme.small
                wrapMode: Text.Wrap
            }
            Text {
                visible: root.locked && root.targetName !== ""
                Layout.fillWidth: true
                text: root.target === 59 && !root.isMouse ? "Fn is the keyboard's shift-layer key and can't be remapped (the firmware reserves it)." : "This key turns the shift layer on, so it can't have a shift-layer assignment."
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: Theme.small
                wrapMode: Text.Wrap
            }

            Repeater {
                model: root.categories
                delegate: ColumnLayout {
                    id: cat
                    required property var modelData
                    readonly property bool current: !!root.assignment && (root.assignment.function === modelData.fn || (modelData.alt || []).indexOf(root.assignment.function) >= 0)
                    readonly property bool expanded: root.open === modelData.id
                    Layout.fillWidth: true
                    visible: (!modelData.mouseOnly || root.isMouse) && !root.locked && (root.listOpen || expanded)
                    spacing: 8

                    Rectangle {
                        visible: root.listOpen
                        Layout.fillWidth: true
                        Layout.preferredHeight: 32
                        color: ch.hovered ? "#2d2d2d" : "transparent"
                        opacity: root.target < 0 ? 0.4 : 1
                        Text {
                            anchors.left: parent.left
                            anchors.leftMargin: 4
                            anchors.verticalCenter: parent.verticalCenter
                            text: cat.modelData.name.toUpperCase()
                            color: cat.expanded || (root.open === "" && cat.current) ? Theme.accent : Theme.text
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
                            onTapped: root.openCategory(cat.modelData.id)
                        }
                    }

                    // ---- editors
                    ColumnLayout {
                        visible: cat.expanded && !root.listOpen
                        Layout.fillWidth: true
                        Layout.leftMargin: 4
                        spacing: 6

                        Text {
                            visible: cat.modelData.later !== undefined
                            text: cat.modelData.later || ""
                            color: Theme.textMuted
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }

                        // Default
                        Text {
                            visible: cat.modelData.id === "default"
                            Layout.fillWidth: true
                            readonly property var d: root.defaultFor(root.target)
                            text: d ? "Default: " + KeyNames.describe({ function: d.function, data: d.data, label: "" }) : "Factory default for this button isn't known yet."
                            color: Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                            wrapMode: Text.Wrap
                        }
                        Item {
                            visible: cat.modelData.id === "default"
                            implicitWidth: restoreBtn.implicitWidth
                            implicitHeight: restoreBtn.implicitHeight
                            SynButton {
                                id: restoreBtn
                                objectName: "restoreDefault"
                                anchors.fill: parent
                                text: "Reset to default"
                                primary: true
                                enabled: !root.readOnly && root.defaultFor(root.target) !== null
                                onClicked: {
                                    const d = root.defaultFor(root.target);
                                    root.save(d.function, d.data);
                                }
                            }
                            HoverHandler { id: restoreHover }
                            QQC2.ToolTip {
                                visible: restoreHover.hovered && root.defaultFor(root.target) === null
                                text: KeyNames.defaultUnknownTip
                            }
                        }

                        // Keyboard function: press the key to assign.
                        Rectangle {
                            id: capture
                            visible: cat.modelData.id === "keyboard"
                            Layout.fillWidth: true
                            Layout.preferredHeight: 44
                            color: Theme.card
                            radius: 2
                            border.color: activeFocus ? Theme.accent : Theme.border
                            focus: visible
                            Component.onCompleted: if (cat.modelData.id === "keyboard") root.captureItem = capture
                            Text {
                                anchors.centerIn: parent
                                text: root.captured ? KeyNames.legend({ function: 2, data: root.captured }) : (capture.activeFocus ? "Press a key or combination…" : "Click here, then press a key")
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
                                    root.captured = d;
                                }
                                ev.accepted = true;
                            }
                        }
                        // Or pick from the list (keys you can't press, e.g. F13–F24, numpad).
                        RowLayout {
                            visible: cat.modelData.id === "keyboard"
                            Layout.fillWidth: true
                            spacing: 6
                            SynDropdown {
                                id: keyPick
                                Layout.fillWidth: true
                                readonly property var keys: Object.keys(KeyNames.hid).map(Number).filter(h => h < 0xe0).sort((a, b) => a - b)
                                model: ["Choose a key…"].concat(keys.map(h => KeyNames.hidName(h)))
                                onActivated: i => {
                                    if (i > 0) {
                                        const m = (modCtrl.checked ? 1 : 0) | (modShift.checked ? 2 : 0) | (modAlt.checked ? 4 : 0) | (modMeta.checked ? 8 : 0);
                                        root.captured = [m, keys[i - 1]];
                                    }
                                }
                            }
                        }
                        RowLayout {
                            visible: cat.modelData.id === "keyboard"
                            spacing: 2
                            SynCheckBox { id: modCtrl; text: "Ctrl" }
                            SynCheckBox { id: modShift; text: "Shift" }
                            SynCheckBox { id: modAlt; text: "Alt" }
                            SynCheckBox { id: modMeta; text: "Meta" }
                        }
                        RowLayout {
                            visible: cat.modelData.id === "keyboard" || cat.modelData.id === "mouse"
                            spacing: 8
                            SynCheckBox {
                                text: "Turbo"
                                checked: root.turbo
                                onToggled: root.turbo = checked
                            }
                            QQC2.SpinBox {
                                enabled: root.turbo
                                from: 1
                                to: 30
                                value: root.turboRate
                                onValueModified: root.turboRate = value
                            }
                            Text {
                                text: "per second"
                                color: Theme.textDim
                                font.family: Theme.font
                                font.pixelSize: Theme.small
                            }
                        }

                        // Mouse function
                        Repeater {
                            model: cat.modelData.id === "mouse" ? [["Left click", 1], ["Right click", 2], ["Scroll click", 3], ["Mouse button 4", 4], ["Mouse button 5", 5], ["Scroll up", 9], ["Scroll down", 10], ["Scroll left", 104], ["Scroll right", 105], ["Double click", -1]] : []
                            delegate: Option {
                                required property var modelData
                                label: modelData[0]
                                on: !!root.assignment && (modelData[1] === -1 ? root.assignment.function === 11 : ([1, 14].indexOf(root.assignment.function) >= 0 && root.assignment.data[0] === modelData[1]))
                                onPicked: modelData[1] === -1 ? root.save(11, [1]) : (root.turbo ? root.save(14, [modelData[1]].concat(root.turboData())) : root.save(1, [modelData[1]]))
                            }
                        }

                        // Sensitivity
                        Repeater {
                            model: cat.modelData.id === "sensitivity" ? [["Sensitivity stage up", 1], ["Sensitivity stage down", 2], ["Cycle up sensitivity stages", 6], ["Cycle down sensitivity stages", 7]] : []
                            delegate: Option {
                                required property var modelData
                                label: modelData[0]
                                on: root.assignment && root.assignment.function === 6 && root.assignment.data[0] === modelData[1]
                                onPicked: root.save(6, [modelData[1]])
                            }
                        }
                        RowLayout {
                            visible: cat.modelData.id === "sensitivity"
                            spacing: 8
                            Text {
                                text: "Clutch"
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
                                text: "Set"
                                implicitWidth: 64
                                enabled: !root.readOnly
                                onClicked: root.save(6, [5, clutchDpi.value >> 8, clutchDpi.value & 255, clutchDpi.value >> 8, clutchDpi.value & 255])
                            }
                        }
                        RowLayout {
                            visible: cat.modelData.id === "sensitivity"
                            spacing: 8
                            Text {
                                text: "Jump to stage"
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
                                text: "Set"
                                implicitWidth: 64
                                enabled: !root.readOnly
                                onClicked: root.save(6, [3, jumpStage.value])
                            }
                        }

                        // Macro: pick from the library + playback option
                        ColumnLayout {
                            visible: cat.modelData.id === "macro"
                            Layout.fillWidth: true
                            spacing: 8
                            Text {
                                visible: root.macroLib.length === 0
                                Layout.fillWidth: true
                                text: "No macros yet. Record one in the Macros module (Home → Macros)."
                                color: Theme.textDim
                                font.family: Theme.font
                                font.pixelSize: Theme.small
                                wrapMode: Text.Wrap
                            }
                            Text {
                                visible: root.macroLib.length > 0
                                text: "Macro"
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
                                text: "Playback option"
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
                                    text: "Times"
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
                                text: "Manage macros"
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
                        Repeater {
                            model: cat.modelData.id === "media" ? [["Play / Pause", 0xcd], ["Next track", 0xb5], ["Previous track", 0xb6], ["Stop", 0xb7], ["Mute", 0xe2], ["Volume up", 0xe9], ["Volume down", 0xea]] : []
                            delegate: Option {
                                required property var modelData
                                label: modelData[0]
                                on: root.assignment && root.assignment.function === 10 && root.assignment.data[1] === modelData[1]
                                onPicked: root.save(10, [0, modelData[1]])
                            }
                        }

                        // Switch profile
                        Repeater {
                            model: cat.modelData.id === "profile" ? [["Next profile", 1], ["Previous profile", 2], ["Cycle forward", 4], ["Cycle backward", 5]] : []
                            delegate: Option {
                                required property var modelData
                                label: modelData[0]
                                on: !!root.assignment && root.assignment.function === 7 && root.assignment.data[0] === modelData[1]
                                onPicked: root.save(7, [modelData[1]])
                            }
                        }
                        RowLayout {
                            visible: cat.modelData.id === "profile"
                            spacing: 8
                            Text {
                                text: "Switch to profile"
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
                                text: "Set"
                                implicitWidth: 64
                                enabled: !root.readOnly
                                onClicked: root.save(7, [3, profileTarget.value])
                            }
                        }

                        // Hypershift
                        Text {
                            visible: cat.modelData.id === "hypershift"
                            Layout.fillWidth: true
                            text: "While this is held, every key and button uses its shift-layer assignment (edit those on the SHIFT LAYER tab). Razer's own software calls this HyperShift."
                            color: Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                            wrapMode: Text.Wrap
                        }
                        SynButton {
                            visible: cat.modelData.id === "hypershift"
                            text: "Make this the shift-layer key"
                            primary: true
                            enabled: !root.readOnly && root.keyLayer === 0
                            onClicked: root.save(12, [1])
                        }

                        // System keys
                        Repeater {
                            model: cat.modelData.id === "system" ? [["Power", 129], ["Sleep", 130], ["Wake", 131]] : []
                            delegate: Option {
                                required property var modelData
                                label: modelData[0]
                                on: !!root.assignment && root.assignment.function === 9 && root.assignment.data[0] === modelData[1]
                                onPicked: root.save(9, [modelData[1]])
                            }
                        }

                        // Scroll wheel (mice)
                        Repeater {
                            model: cat.modelData.id === "scroll" ? [["Tactile / free-spin toggle", 1], ["Scroll stage up", 2], ["Scroll stage down", 3], ["Cycle scroll stages up", 4], ["Cycle scroll stages down", 5]] : []
                            delegate: Option {
                                required property var modelData
                                label: modelData[0]
                                on: !!root.assignment && root.assignment.function === 18 && root.assignment.data[0] === modelData[1]
                                onPicked: root.save(18, [modelData[1]])
                            }
                        }

                        // Shortcuts (key combinations)
                        Repeater {
                            model: cat.modelData.id === "shortcuts" ? KeyNames.shortcuts : []
                            delegate: Option {
                                required property var modelData
                                label: modelData[0]
                                on: !!root.assignment && root.assignment.function === 2 && root.assignment.data[0] === modelData[1] && root.assignment.data[1] === modelData[2]
                                onPicked: root.save(2, [modelData[1], modelData[2]])
                            }
                        }

                        // Launch a program or a website (needs tsubad running).
                        ColumnLayout {
                            visible: cat.modelData.id === "launch"
                            Layout.fillWidth: true
                            spacing: 8
                            ModeTabs {
                                id: launchKind
                                minTabWidth: 110
                                model: ["Program", "Website"]
                                currentIndex: root.launchMode
                                onActivated: i => root.launchMode = i
                            }
                            QQC2.TextField {
                                id: launchTarget
                                Layout.fillWidth: true
                                placeholderText: root.launchMode === 0 ? "Command, e.g. firefox or steam" : "https://…"
                            }
                            SynButton {
                                text: "Assign"
                                primary: true
                                enabled: launchTarget.text.trim() !== "" && !root.readOnly
                                onClicked: root.assignAction(root.launchMode === 0 ? { type: "launch", command: launchTarget.text.trim() } : { type: "url", url: (/^[a-z]+:/i.test(launchTarget.text.trim()) ? "" : "https://") + launchTarget.text.trim() })
                            }
                            Text {
                                Layout.fillWidth: true
                                text: "Runs on your PC while Tsuba's service is running."
                                color: Theme.textMuted
                                font.family: Theme.font
                                font.pixelSize: 11
                                wrapMode: Text.Wrap
                            }
                        }

                        // Inter-device: control another Razer device from this key/button.
                        ColumnLayout {
                            visible: cat.modelData.id === "interdevice"
                            Layout.fillWidth: true
                            spacing: 8
                            Text {
                                visible: root.otherDevices.length === 0
                                text: "Connect another Razer device to use this."
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
                                delegate: Option {
                                    required property var modelData
                                    label: modelData[0]
                                    onPicked: {
                                        const op = modelData[1] || { op: "dpi_clutch", dpi: interClutch.value };
                                        root.assignAction({ type: "device", target: root.otherDevices[root.interTarget].pid, op: op });
                                    }
                                }
                            }
                            RowLayout {
                                visible: root.otherDevices.length > 0
                                spacing: 8
                                Text {
                                    text: "Clutch DPI"
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

                        // Lighting (Chroma functions): this device or another one.
                        ColumnLayout {
                            visible: cat.modelData.id === "lighting"
                            Layout.fillWidth: true
                            spacing: 8
                            readonly property var targets: [{ pid: root.pid, name: "This device" }].concat(root.otherDevices.map(d => ({ pid: d.pid, name: d.name.replace(" (wired)", "") })))
                            SynDropdown {
                                id: lightTarget
                                Layout.fillWidth: true
                                model: parent.targets.map(t => t.name)
                            }
                            Repeater {
                                model: cat.modelData.id === "lighting" ? [
                                    ["Next effect", { op: "effect_next" }], ["Previous effect", { op: "effect_previous" }],
                                    ["Brightness up", { op: "brightness_up" }], ["Brightness down", { op: "brightness_down" }],
                                    ["Lighting on / off", { op: "lighting_toggle" }],
                                    ["Spectrum cycling", { op: "effect", effect: { type: "spectrum" } }],
                                    ["Wave", { op: "effect", effect: { type: "wave", direction: 1, speed: 40 } }],
                                    ["Breathing (random)", { op: "effect", effect: { type: "breathing", colours: [] } }],
                                    ["Starlight", { op: "effect", effect: { type: "starlight", speed: 2, colours: [] } }],
                                    ["Lighting off", { op: "effect", effect: { type: "off" } }]
                                ] : []
                                delegate: Option {
                                    required property var modelData
                                    label: modelData[0]
                                    onPicked: root.assignAction({ type: "device", target: lightTarget.parent.targets[lightTarget.currentIndex].pid, op: modelData[1] })
                                }
                            }
                        }

                        // Audio (desktop sound through PipeWire)
                        Repeater {
                            model: cat.modelData.id === "audio" ? [["Volume up", "volume_up"], ["Volume down", "volume_down"], ["Mute / unmute", "mute_toggle"], ["Mic mute / unmute", "mic_mute_toggle"], ["Next audio output", "next_output"], ["Previous audio output", "previous_output"]] : []
                            delegate: Option {
                                required property var modelData
                                label: modelData[0]
                                onPicked: root.assignAction({ type: "audio", op: modelData[1] })
                            }
                        }

                        // Text: typed by an on-board macro, so it works without Tsuba running.
                        ColumnLayout {
                            visible: cat.modelData.id === "text"
                            Layout.fillWidth: true
                            spacing: 8
                            QQC2.TextField {
                                id: textInput
                                Layout.fillWidth: true
                                placeholderText: "Text to type"
                            }
                            RowLayout {
                                spacing: 8
                                Text {
                                    text: "Delay between characters (ms)"
                                    color: Theme.text
                                    font.family: Theme.font
                                    font.pixelSize: Theme.small
                                }
                                QQC2.SpinBox {
                                    id: textDelay
                                    from: 0
                                    to: 500
                                    value: 10
                                    editable: true
                                }
                            }
                            SynButton {
                                text: "Assign text"
                                primary: true
                                enabled: textInput.text.length > 0 && !root.readOnly
                                onClicked: root.assignText(textInput.text, textDelay.value)
                            }
                            Text {
                                Layout.fillWidth: true
                                text: "Typed with a US keyboard layout. Saved as a macro on the device."
                                color: Theme.textMuted
                                font.family: Theme.font
                                font.pixelSize: 11
                                wrapMode: Text.Wrap
                            }
                        }

                        // Disable
                        SynButton {
                            visible: cat.modelData.id === "disable"
                            text: "Disable this " + (root.isMouse ? "button" : "key")
                            enabled: !root.readOnly
                            onClicked: root.save(0, [])
                        }
                    }
                }
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
}
