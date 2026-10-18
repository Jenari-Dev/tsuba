import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// Macros module (Synapse layout): the macro library on the left, the recorder and event
// editor on the right. Macros live on the PC and are written to a device's on-board memory
// when a key is bound to one (CUSTOMIZE → Macro).
Rectangle {
    id: page

    required property var backend

    property var lib: []
    property var devs: ({})
    property var deviceNames: ({})
    // Which profiles use which macro (null until the daemon answered).
    property var usage: null
    property alias lp: provider
    LibProvider {
        id: provider
    }
    readonly property var deviceOnlyList: deviceOnly()

    // The macro being edited.
    property string guid: ""
    property string macroName: ""
    property var events: []
    property int sel: -1
    property bool dirty: false
    property bool editing: false
    // An on-board macro shown read-only (not in the library yet): {pid, id, name}.
    property var devView: null
    readonly property bool readOnly: devView !== null
    // Long keyboard macros can come back cut short from the device: unbalanced presses/releases
    // or a trailing delay suggest the list stops early.
    readonly property bool looksIncomplete: {
        if (!readOnly || events.length === 0) {
            return false;
        }
        const down = {};
        for (const e of events) {
            if (e.type === "key" || e.type === "button") {
                const k = e.type + ":" + (e.type === "key" ? e.hid : e.button);
                down[k] = (down[k] || 0) + (e.down ? 1 : -1);
            }
        }
        return Object.values(down).some(v => v !== 0) || events[events.length - 1].type === "delay";
    }

    // A "Run command" event without a command can't be saved.
    readonly property bool hasEmptyCommand: events.some(e => e.type === "run" && e.command.trim() === "")

    property bool recording: false
    property int delayMode: 0          // 0 recorded, 1 fixed, 2 none
    // What to record: 0 keys and mouse, 1 keys only, 2 mouse only. Pointer moves are recorded
    // as "Move cursor" events only when asked for.
    property int recordFilter: 0
    property bool trackMoves: false
    property double lastMove: 0
    property int fixedDelay: 50
    property double lastTime: 0
    property string result: ""

    readonly property var buttonNames: ["", "Left click", "Right click", "Middle click", "Button 4", "Button 5"]

    color: Theme.bg

    function load() {
        const s = backend.macros();
        if (s === "") {
            result = "Failed: " + backend.status;
            return;
        }
        const j = JSON.parse(s);
        lib = j.library;
        devs = j.devices;
        const names = {};
        for (const pid in devs) {
            for (const m of devs[pid].macros) {
                names[m.id] = m.name;
            }
        }
        KeyNames.macroNames = names;
        const list = JSON.parse(backend.devices || "[]");
        const dn = {};
        list.forEach(d => dn[d.pid] = d.name.replace(" (wired)", ""));
        deviceNames = dn;
        usage = provider.json("MacroUsage", [], null);
    }

    function openMacro(m) {
        stopRecording();
        devView = null;
        guid = m.guid;
        macroName = m.name;
        events = m.events.slice();
        sel = -1;
        dirty = false;
        editing = true;
        resetEvHist();
        result = "";
    }

    // Show an on-board-only macro read-only, with an "Import to library" button.
    function openDeviceMacro(pid, m) {
        stopRecording();
        devView = { pid: pid, id: m.id, name: m.name };
        guid = m.guid || "";
        macroName = m.name;
        events = (m.events || []).slice();
        sel = -1;
        dirty = false;
        editing = true;
        resetEvHist();
        result = "";
    }

    function newMacro() {
        stopRecording();
        devView = null;
        guid = "";
        macroName = "Macro " + (lib.length + 1);
        events = [];
        sel = -1;
        dirty = true;
        editing = true;
        resetEvHist();
        result = "";
    }

    function save() {
        stopRecording();
        const g = backend.save_macro(JSON.stringify({ guid: guid, name: macroName, events: events }));
        if (g === "") {
            result = "Failed: " + backend.status;
            return;
        }
        guid = g;
        dirty = false;
        result = "Saved";
        load();
    }

    function removeMacro() {
        if (guid === "") {
            editing = false;
            return;
        }
        if (backend.delete_macro(guid)) {
            editing = false;
            result = "";
            load();
        } else {
            result = "Failed: " + backend.status;
        }
    }

    function importMacro(pid, id) {
        const g = backend.import_macro(pid, id);
        if (g === "") {
            result = "Import failed: " + (backend.status !== "" ? backend.status : "the daemon returned no macro");
            return;
        }
        const name = macroName;
        load();
        const m = lib.find(x => x.guid === g);
        if (m) {
            openMacro(m);
            result = "Imported \u201c" + m.name + "\u201d to the library";
        } else {
            result = "Imported \u201c" + name + "\u201d, but it isn't in the library list yet";
        }
    }

    // Where a library macro is on board: ["BlackWidow V4 Pro", ...].
    function placesOf(g) {
        const out = [];
        for (const pid in devs) {
            if (devs[pid].macros.some(m => m.guid === g)) {
                out.push(deviceNames[pid] || pid);
            }
        }
        return out;
    }

    function deviceOnly() {
        const out = [];
        for (const pid in devs) {
            for (const m of devs[pid].macros) {
                if (!lib.some(x => x.guid === m.guid)) {
                    out.push({ pid: pid, m: m });
                }
            }
        }
        return out;
    }

    function describe(e) {
        switch (e.type) {
        case "key":
            return KeyNames.hidName(e.hid);
        case "button":
            return buttonNames[e.button] || ("Button " + e.button);
        case "scroll":
            return e.up ? "Scroll up" : "Scroll down";
        case "delay":
            return e.ms >= 1000 ? (e.ms / 1000).toFixed(3) + " s" : e.ms + " ms";
        case "cursor":
            return i18n("Move cursor to %1, %2", e.x, e.y);
        case "media":
            return (KeyNames.media[e.usage] || ("Media key 0x" + e.usage.toString(16)));
        case "power":
            return KeyNames.powerKeys[e.code] || ("System key " + e.code);
        case "run":
            return e.command.trim() === "" ? i18n("Run command (not set)") : i18n("Run: %1", e.command);
        default:
            return "Event 0x" + e.op.toString(16);
        }
    }

    function glyph(e) {
        if (e.type === "delay") {
            return "⏱";
        }
        if (e.type === "key" || e.type === "button" || e.type === "media" || e.type === "power") {
            return e.down ? "↓" : "↑";
        }
        return e.type === "scroll" ? "⇕" : "•";
    }

    // Undo / redo over the event list, plus a multi-selection for bulk delete.
    property var evHist: []
    property int evPos: -1
    property var marked: ({})
    readonly property int markedCount: Object.keys(marked).length
    readonly property int totalMs: events.reduce((t, e) => t + (e.type === "delay" ? e.ms : 0), 0)
    function resetEvHist() {
        evHist = [JSON.stringify(events)];
        evPos = 0;
        marked = ({});
    }
    function fmtMs(ms) {
        return ms >= 1000 ? (ms / 1000).toFixed(3) + " s" : ms + " ms";
    }
    function setEvents(list, newSel) {
        events = list;
        sel = newSel;
        dirty = true;
        marked = ({});
        const h = evHist.slice(0, evPos + 1);
        h.push(JSON.stringify(list));
        evHist = h.slice(-200);
        evPos = evHist.length - 1;
    }
    function undoEv() {
        if (evPos > 0) {
            evPos--;
            events = JSON.parse(evHist[evPos]);
            sel = -1;
            dirty = true;
            marked = ({});
        }
    }
    function redoEv() {
        if (evPos >= 0 && evPos < evHist.length - 1) {
            evPos++;
            events = JSON.parse(evHist[evPos]);
            sel = -1;
            dirty = true;
            marked = ({});
        }
    }
    function mark(i, on) {
        const m = Object.assign({}, marked);
        if (on) {
            m[i] = true;
        } else {
            delete m[i];
        }
        marked = m;
    }
    function markAll(on) {
        const m = {};
        if (on) {
            events.forEach((e, i) => m[i] = true);
        }
        marked = m;
    }
    function removeMarked() {
        setEvents(events.filter((e, i) => !marked[i]), -1);
    }
    function insertKind(kind) {
        switch (kind) {
        case "delay":
            insert([{ type: "delay", ms: 50 }]);
            break;
        case "key":
            insert([{ type: "key", hid: 0x04, down: true }, { type: "key", hid: 0x04, down: false }]);
            break;
        case "button":
            insert([{ type: "button", button: 1, down: true }, { type: "button", button: 1, down: false }]);
            break;
        case "scroll":
            insert([{ type: "scroll", up: true }]);
            break;
        case "media":
            insert([{ type: "media", usage: 0xcd, down: true }, { type: "media", usage: 0xcd, down: false }]);
            break;
        case "power":
            insert([{ type: "power", code: 0x83, down: true }, { type: "power", code: 0x83, down: false }]);
            break;
        case "cursor":
            insert([{ type: "cursor", x: 960, y: 540 }]);
            break;
        case "run":
            insert([{ type: "run", command: "" }]);
            break;
        }
    }

    // Small line icons (our own drawing) for macro events.
    component MIcon: Canvas {
        id: mi
        property string kind: "delay"
        property color ink: Theme.text
        implicitWidth: 18
        implicitHeight: 18
        width: 18
        height: 18
        onKindChanged: requestPaint()
        onInkChanged: requestPaint()
        onPaint: {
            const c = getContext("2d");
            c.reset();
            c.strokeStyle = ink;
            c.lineWidth = 1.4;
            c.lineCap = "round";
            c.lineJoin = "round";
            const ln = (a, b, d, e) => {
                c.beginPath();
                c.moveTo(a, b);
                c.lineTo(d, e);
                c.stroke();
            };
            switch (kind) {
            case "delay":    // clock
                c.beginPath();
                c.arc(9, 9, 6.5, 0, Math.PI * 2);
                c.stroke();
                ln(9, 5, 9, 9);
                ln(9, 9, 12, 11);
                break;
            case "key":      // keycap
                c.strokeRect(2.5, 4, 13, 10);
                c.strokeRect(5.5, 6.5, 7, 5);
                break;
            case "button":   // mouse
                c.beginPath();
                c.moveTo(4.5, 7);
                c.bezierCurveTo(4.5, 1.5, 13.5, 1.5, 13.5, 7);
                c.lineTo(13.5, 11);
                c.bezierCurveTo(13.5, 16.5, 4.5, 16.5, 4.5, 11);
                c.closePath();
                c.stroke();
                ln(9, 2.5, 9, 7);
                break;
            case "scroll":   // wheel with arrows
                c.strokeRect(6.5, 6, 5, 6);
                ln(9, 1.5, 9, 4.5);
                ln(7.5, 3, 9, 1.5);
                ln(10.5, 3, 9, 1.5);
                ln(9, 13.5, 9, 16.5);
                ln(7.5, 15, 9, 16.5);
                ln(10.5, 15, 9, 16.5);
                break;
            case "media":    // play triangle in a circle
                c.beginPath();
                c.arc(9, 9, 6.5, 0, Math.PI * 2);
                c.stroke();
                c.beginPath();
                c.moveTo(7.5, 6);
                c.lineTo(12, 9);
                c.lineTo(7.5, 12);
                c.closePath();
                c.stroke();
                break;
            case "power":    // power symbol
                c.beginPath();
                c.arc(9, 10, 5.5, -Math.PI * 0.32, Math.PI * 1.32);
                c.stroke();
                ln(9, 2.5, 9, 9);
                break;
            case "cursor":   // arrow pointer
                c.beginPath();
                c.moveTo(5, 3);
                c.lineTo(5, 14);
                c.lineTo(8, 11);
                c.lineTo(11, 15);
                c.lineTo(12.5, 14);
                c.lineTo(9.5, 10);
                c.lineTo(13, 10);
                c.closePath();
                c.stroke();
                break;
            case "run":      // prompt
                ln(3.5, 5, 8, 9);
                ln(8, 9, 3.5, 13);
                ln(10, 13.5, 15, 13.5);
                break;
            default:
                c.beginPath();
                c.arc(9, 9, 2, 0, Math.PI * 2);
                c.stroke();
            }
        }
    }
    // Compact outlined toolbar button.
    component ToolPill: Rectangle {
        id: tp
        property string label: ""
        signal clicked
        implicitWidth: tl.implicitWidth + 20
        implicitHeight: 28
        color: ph.hovered && enabled ? "#1affffff" : "transparent"
        border.color: ph.hovered && enabled ? Theme.accent : Theme.border
        opacity: enabled ? 1 : 0.4
        Text {
            id: tl
            anchors.centerIn: parent
            text: tp.label
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.small
        }
        HoverHandler {
            id: ph
            cursorShape: Qt.PointingHandCursor
        }
        TapHandler {
            enabled: tp.enabled
            onTapped: tp.clicked()
        }
    }

    function insert(evs) {
        const at = sel >= 0 ? sel + 1 : events.length;
        const l = events.slice();
        l.splice(at, 0, ...evs);
        setEvents(l, at + evs.length - 1);
    }

    function update(i, e) {
        const l = events.slice();
        l[i] = e;
        setEvents(l, i);
    }

    function move(d) {
        const j = sel + d;
        if (sel < 0 || j < 0 || j >= events.length) {
            return;
        }
        const l = events.slice();
        [l[sel], l[j]] = [l[j], l[sel]];
        setEvents(l, j);
    }

    function removeEvent() {
        if (sel < 0) {
            return;
        }
        const l = events.slice();
        l.splice(sel, 1);
        setEvents(l, Math.min(sel, l.length - 1));
    }

    // ---- Recorder
    function startRecording() {
        recording = true;
        lastTime = 0;
        recorder.forceActiveFocus();
    }

    function stopRecording() {
        recording = false;
    }

    function record(e) {
        const keyLike = e.type === "key" || e.type === "media" || e.type === "power";
        if ((recordFilter === 1 && !keyLike) || (recordFilter === 2 && keyLike)) {
            return;
        }
        const now = Date.now();
        const add = [];
        if (events.length > 0 && lastTime > 0) {
            if (delayMode === 0) {
                add.push({ type: "delay", ms: Math.max(1, Math.round(now - lastTime)) });
            } else if (delayMode === 1) {
                add.push({ type: "delay", ms: fixedDelay });
            }
        }
        lastTime = now;
        add.push(e);
        setEvents(events.concat(add), events.length + add.length - 1);
        eventList.positionViewAtEnd();
    }

    Component.onCompleted: {
        load();
        if (backend.env_var("TSUBA_SYNAPSE") !== "") {   // headless UI check
            synapseDlg.open("macros");
        }
        // Headless UI check: open the first on-board macro in the editor (nothing is saved).
        if (backend.env_var("TSUBA_DEMO") === "1") {
            const d = deviceOnly();
            if (d.length) {
                openDeviceMacro(d[d.length - 1].pid, d[d.length - 1].m);
                sel = 2;
            }
        }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // ---- Library
        Rectangle {
            Layout.preferredWidth: 300
            Layout.fillHeight: true
            color: Theme.panel
            border.color: Theme.line

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 16
                spacing: 10

                RowLayout {
                    Layout.fillWidth: true
                    SectionLabel {
                        caption: "Macros"
                    }
                    HelpButton {
                        Layout.leftMargin: 6
                        size: 16
                        text: "Macros are recorded key, click and scroll sequences with delays. Press Record, then type or click in this window; edit or reorder events afterwards and click Save. Bind a macro to a key from a device's Customize tab. Macros are stored on this PC and copied to the device when bound."
                    }
                    Item {
                        Layout.fillWidth: true
                    }
                    SynButton {
                        objectName: "macroSynapseImport"
                        text: "Import"
                        implicitWidth: 80
                        onClicked: synapseDlg.open("macros")
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.text: "Import from Synapse…"
                    }
                    SynButton {
                        text: "+ New"
                        implicitWidth: 80
                        enabled: !page.backend.readonly
                        onClicked: page.newMacro()
                    }
                }

                MacroList {
                    id: macroList
                    objectName: "macroList"
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    backend: page.backend
                    lp: page.lp
                    lib: page.lib
                    devs: page.devs
                    deviceNames: page.deviceNames
                    deviceOnlyList: page.deviceOnlyList
                    usage: page.usage
                    activeGuid: page.guid
                    editing: page.editing
                    devView: page.devView
                    onOpenMacro: m => page.openMacro(m)
                    onOpenDeviceMacro: (pid, m) => page.openDeviceMacro(pid, m)
                    onImportDeviceMacro: (pid, id) => page.importMacro(pid, id)
                    onChanged: page.load()
                    onDuplicated: g => {
                        const m = page.lib.find(x => x.guid === g);
                        if (m) {
                            page.openMacro(m);
                        }
                    }
                    onRemoved: g => {
                        if (page.guid === g) {
                            page.editing = false;
                        }
                    }
                    onRenamed: (g, n) => {
                        if (page.guid === g && !page.dirty) {
                            page.macroName = n;
                        }
                    }
                }

                Text {
                    objectName: "libraryResult"
                    visible: page.result !== "" && !page.editing
                    Layout.fillWidth: true
                    text: page.result
                    color: page.result.indexOf("failed") >= 0 || page.result.startsWith("Failed") ? Theme.warn : Theme.accent
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    wrapMode: Text.Wrap
                }

                // Memory per device
                Repeater {
                    model: Object.keys(page.devs)
                    delegate: Text {
                        required property string modelData
                        readonly property var d: page.devs[modelData]
                        Layout.fillWidth: true
                        text: `${page.deviceNames[modelData] || modelData}: ${d.macros.length} on board · ${Math.round(d.available / 1024)} KB free`
                        color: Theme.textMuted
                        font.family: Theme.font
                        font.pixelSize: 11
                        elide: Text.ElideRight
                    }
                }
            }
        }

        // ---- Editor
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            Column {
                visible: !page.editing
                anchors.centerIn: parent
                spacing: 12
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: "MACROS"
                    color: Theme.text
                    font.family: Theme.font
                    font.pixelSize: Theme.heading
                    font.letterSpacing: 2
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: 460
                    horizontalAlignment: Text.AlignHCenter
                    text: "Record keystrokes, mouse clicks and delays, then bind a macro to any key or button from a device's CUSTOMIZE tab."
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.Wrap
                }
                SynButton {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: "+ New macro"
                    primary: true
                    enabled: !page.backend.readonly
                    onClicked: page.newMacro()
                }
            }

            RowLayout {
                visible: page.editing
                anchors.fill: parent
                anchors.margins: 20
                spacing: 16

                // ---- ADD: insertable items
                SynCard {
                    objectName: "addCard"
                    visible: !page.readOnly
                    Layout.fillWidth: false
                    Layout.preferredWidth: 180
                    Layout.alignment: Qt.AlignTop
                    title: "Add"
                    showHelp: false
                    padding: 16
                    Repeater {
                        model: [
                            { kind: "delay", label: "Delay" },
                            { kind: "key", label: "Keyboard key" },
                            { kind: "button", label: "Mouse button" },
                            { kind: "scroll", label: "Scroll wheel" },
                            { kind: "media", label: i18n("Media key") },
                            { kind: "power", label: i18n("System key") },
                            { kind: "cursor", label: i18n("Cursor move") },
                            { kind: "run", label: i18n("Run command") }
                        ]
                        delegate: Rectangle {
                            required property var modelData
                            Layout.fillWidth: true
                            Layout.preferredHeight: 40
                            color: ah.hovered ? "#1affffff" : "transparent"
                            border.color: ah.hovered ? Theme.accent : Theme.borderSoft
                            Row {
                                anchors.verticalCenter: parent.verticalCenter
                                anchors.left: parent.left
                                anchors.leftMargin: 10
                                spacing: 10
                                MIcon {
                                    anchors.verticalCenter: parent.verticalCenter
                                    kind: modelData.kind
                                    ink: ah.hovered ? Theme.accent : Theme.text
                                }
                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: modelData.label
                                    color: ah.hovered ? Theme.accent : Theme.text
                                    font.family: Theme.font
                                    font.pixelSize: Theme.small
                                }
                            }
                            HoverHandler {
                                id: ah
                                cursorShape: Qt.PointingHandCursor
                            }
                            TapHandler {
                                onTapped: page.insertKind(modelData.kind)
                            }
                        }
                    }
                    Text {
                        Layout.fillWidth: true
                        text: "New items go after the selected row, or at the end."
                        color: Theme.textMuted
                        font.family: Theme.font
                        font.pixelSize: 11
                        wrapMode: Text.Wrap
                    }
                }

                // ---- Editor card
                Rectangle {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.minimumWidth: 360
                    color: Theme.card
                    radius: 5

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 16
                        spacing: 12

                        // Name + delete
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 12
                            Text {
                                visible: page.readOnly
                                text: page.macroName
                                color: Theme.textBright
                                font.family: Theme.font
                                font.pixelSize: Theme.heading
                                elide: Text.ElideRight
                                Layout.maximumWidth: 320
                            }
                            Text {
                                visible: page.readOnly
                                objectName: "onDeviceTag"
                                text: "ON DEVICE ONLY \u00b7 READ-ONLY"
                                color: Theme.textDim
                                font.family: Theme.font
                                font.pixelSize: 11
                                font.letterSpacing: 1
                            }
                            QQC2.TextField {
                                visible: !page.readOnly
                                Layout.preferredWidth: 320
                                text: page.macroName
                                placeholderText: "Macro name"
                                font.family: Theme.font
                                font.pixelSize: Theme.heading
                                onTextEdited: {
                                    page.macroName = text;
                                    page.dirty = true;
                                }
                            }
                            Text {
                                visible: page.dirty
                                text: "UNSAVED"
                                color: Theme.warn
                                font.family: Theme.font
                                font.pixelSize: 11
                                font.letterSpacing: 1
                            }
                            Item {
                                Layout.fillWidth: true
                            }
                            SynButton {
                                visible: !page.readOnly
                                text: "Delete"
                                enabled: !page.backend.readonly
                                onClicked: page.removeMacro()
                            }
                            SynButton {
                                objectName: "importButton"
                                visible: page.readOnly
                                text: "Import to library"
                                primary: true
                                implicitWidth: 160
                                onClicked: page.importMacro(page.devView.pid, page.devView.id)
                            }
                        }

                        // Toolbar (also the key-capture surface while recording)
                        Rectangle {
                            id: recorder
                            objectName: "recorderBox"
                            visible: !page.readOnly
                            Layout.fillWidth: true
                            Layout.preferredHeight: 52
                            color: page.recording ? "#22c8323c" : Theme.bg
                            border.color: page.recording ? Theme.danger : (activeFocus ? Theme.accent : Theme.borderSoft)
                            focus: page.recording

                            RowLayout {
                                anchors.fill: parent
                                anchors.leftMargin: 14
                                anchors.rightMargin: 10
                                spacing: 12
                                SynCheckBox {
                                    objectName: "selectAllEvents"
                                    focusPolicy: Qt.NoFocus
                                    enabled: page.events.length > 0
                                    checked: page.events.length > 0 && page.markedCount === page.events.length
                                    onToggled: page.markAll(checked)
                                }
                                Text {
                                    objectName: "totalDuration"
                                    text: "Total  " + page.fmtMs(page.totalMs)
                                    color: Theme.text
                                    font.family: Theme.font
                                    font.pixelSize: Theme.small
                                }
                                Text {
                                    visible: page.markedCount > 0
                                    text: page.markedCount + " selected"
                                    color: Theme.accent
                                    font.family: Theme.font
                                    font.pixelSize: Theme.small
                                }
                                ToolPill {
                                    visible: page.markedCount > 0
                                    label: "Remove selected"
                                    onClicked: page.removeMarked()
                                }
                                Item {
                                    Layout.fillWidth: true
                                }
                                ToolPill {
                                    objectName: "macroUndo"
                                    label: "Undo"
                                    enabled: page.evPos > 0
                                    onClicked: page.undoEv()
                                }
                                ToolPill {
                                    objectName: "macroRedo"
                                    label: "Redo"
                                    enabled: page.evPos >= 0 && page.evPos < page.evHist.length - 1
                                    onClicked: page.redoEv()
                                }
                                // RECORD with a red dot, and a drop-down for the record options.
                                Row {
                                    spacing: 2
                                    SynButton {
                                        id: recordButton
                                        objectName: "recordBtn"
                                        text: page.recording ? "Stop" : "Record"
                                        implicitWidth: 122
                                        enabled: !page.backend.readonly
                                        focusPolicy: Qt.NoFocus
                                        onClicked: page.recording ? page.stopRecording() : page.startRecording()
                                        Rectangle {
                                            x: 14
                                            anchors.verticalCenter: parent.verticalCenter
                                            width: 12
                                            height: 12
                                            radius: page.recording ? 1 : 6
                                            color: Theme.danger
                                        }
                                    }
                                    SynButton {
                                        id: recOptions
                                        objectName: "recordOptions"
                                        text: "▾"
                                        implicitWidth: 34
                                        focusPolicy: Qt.NoFocus
                                        enabled: !page.recording
                                        onClicked: recordPopover.opened ? recordPopover.close() : recordPopover.open()
                                        QQC2.Popup {
                                            id: recordPopover
                                            x: parent.width - width
                                            y: parent.height + 4
                                            width: 300
                                            padding: 14
                                            background: Rectangle {
                                                color: Theme.card
                                                border.color: Theme.accent
                                            }
                                            contentItem: ColumnLayout {
                                                spacing: 10
                                                Text {
                                                    text: "RECORD OPTIONS"
                                                    color: Theme.textDim
                                                    font.family: Theme.font
                                                    font.pixelSize: 11
                                                    font.letterSpacing: 1
                                                }
                                                Text {
                                                    text: "Delays between events"
                                                    color: Theme.text
                                                    font.family: Theme.font
                                                    font.pixelSize: Theme.small
                                                }
                                                ModeTabs {
                                                    minTabWidth: 86
                                                    model: ["Recorded", "Fixed", "None"]
                                                    currentIndex: page.delayMode
                                                    onActivated: i => page.delayMode = i
                                                }
                                                Text {
                                                    text: i18n("What to record")
                                                    color: Theme.text
                                                    font.family: Theme.font
                                                    font.pixelSize: Theme.small
                                                }
                                                ModeTabs {
                                                    objectName: "recordFilter"
                                                    minTabWidth: 86
                                                    model: [i18n("Keys + mouse"), i18n("Keys only"), i18n("Mouse only")]
                                                    currentIndex: page.recordFilter
                                                    onActivated: i => page.recordFilter = i
                                                }
                                                SynCheckBox {
                                                    objectName: "trackMoves"
                                                    text: i18n("Record pointer movement")
                                                    enabled: page.recordFilter !== 1
                                                    checked: page.trackMoves
                                                    onToggled: page.trackMoves = checked
                                                }
                                                RowLayout {
                                                    visible: page.delayMode === 1
                                                    spacing: 10
                                                    Text {
                                                        text: "Fixed delay (ms)"
                                                        color: Theme.text
                                                        font.family: Theme.font
                                                        font.pixelSize: Theme.small
                                                    }
                                                    QQC2.SpinBox {
                                                        from: 1
                                                        to: 60000
                                                        editable: true
                                                        value: page.fixedDelay
                                                        onValueModified: page.fixedDelay = value
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                SynButton {
                                    objectName: "macroSave"
                                    text: "Save"
                                    primary: true
                                    implicitWidth: 90
                                    enabled: page.dirty && page.events.length > 0 && !page.backend.readonly && !page.hasEmptyCommand
                                    onClicked: page.save()
                                }
                            }

                            Keys.onPressed: ev => {
                                if (!page.recording || ev.isAutoRepeat) {
                                    ev.accepted = page.recording;
                                    return;
                                }
                                const h = KeyNames.hidOfKeyEvent(ev);
                                if (h >= 0) {
                                    page.record({ type: "key", hid: h, down: true });
                                }
                                ev.accepted = true;
                            }
                            Keys.onReleased: ev => {
                                if (!page.recording || ev.isAutoRepeat) {
                                    ev.accepted = page.recording;
                                    return;
                                }
                                const h = KeyNames.hidOfKeyEvent(ev);
                                if (h >= 0) {
                                    page.record({ type: "key", hid: h, down: false });
                                }
                                ev.accepted = true;
                            }
                        }
                        Text {
                            visible: page.readOnly
                            objectName: "deviceNote"
                            Layout.fillWidth: true
                            text: page.events.length + " events, " + page.fmtMs(page.totalMs) + " in total. Import it to keep a copy in the library, where you can edit it."
                            color: Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                            wrapMode: Text.Wrap
                        }
                        Text {
                            visible: page.looksIncomplete
                            objectName: "truncatedNote"
                            Layout.fillWidth: true
                            text: "This event list may be incomplete: the device returns long keyboard macros cut short, so the end may be missing. Importing saves only what was read."
                            color: Theme.warn
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                            wrapMode: Text.Wrap
                        }
                        Text {
                            visible: !page.readOnly
                            Layout.fillWidth: true
                            text: page.recording ? "Recording: type, click and scroll anywhere in this window. Click Stop when done." : "Recorded events are added after the selected row (or at the end)."
                            color: page.recording ? Theme.textBright : Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                            wrapMode: Text.Wrap
                        }

                        // Event list
                        Rectangle {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            color: Theme.panel
                            border.color: Theme.borderSoft

                            ListView {
                                id: eventList
                                anchors.fill: parent
                                anchors.margins: 1
                                clip: true
                                model: page.events
                                boundsBehavior: Flickable.StopAtBounds
                                QQC2.ScrollBar.vertical: QQC2.ScrollBar {}
                                delegate: Rectangle {
                                    required property var modelData
                                    required property int index
                                    readonly property bool selected: page.sel === index
                                    readonly property bool isDelay: modelData.type === "delay"
                                    width: eventList.width
                                    height: 36
                                    color: selected ? Theme.accentTint : (index % 2 ? "#0affffff" : "transparent")
                                    border.color: selected ? Theme.accent : "transparent"
                                    RowLayout {
                                        anchors.fill: parent
                                        anchors.leftMargin: 10
                                        anchors.rightMargin: 12
                                        spacing: 10
                                        SynCheckBox {
                                            visible: !page.readOnly
                                            focusPolicy: Qt.NoFocus
                                            checked: !!page.marked[index]
                                            onToggled: page.mark(index, checked)
                                        }
                                        Text {
                                            Layout.preferredWidth: 26
                                            text: index + 1
                                            color: Theme.textMuted
                                            font.family: Theme.font
                                            font.pixelSize: 11
                                        }
                                        MIcon {
                                            kind: modelData.type
                                            ink: isDelay ? Theme.textDim : (selected ? Theme.accent : Theme.text)
                                        }
                                        Text {
                                            Layout.preferredWidth: 14
                                            text: modelData.type === "scroll" ? (modelData.up ? "↑" : "↓") : (modelData.down === undefined ? "" : (modelData.down ? "↓" : "↑"))
                                            color: isDelay || modelData.down === false ? Theme.textDim : Theme.accent
                                            font.pixelSize: 14
                                            font.bold: true
                                        }
                                        Text {
                                            Layout.fillWidth: true
                                            text: page.describe(modelData)
                                            color: isDelay ? Theme.textDim : Theme.text
                                            font.family: Theme.font
                                            font.pixelSize: Theme.fontSize
                                            elide: Text.ElideRight
                                        }
                                    }
                                    TapHandler {
                                        onTapped: page.sel = page.sel === index ? -1 : index
                                    }
                                }
                            }
                            Text {
                                anchors.centerIn: parent
                                visible: page.events.length === 0
                                text: "Press Record, or add items from the list on the left."
                                color: Theme.textDim
                                font.family: Theme.font
                                font.pixelSize: Theme.small
                            }
                        }
                    }
                }

                // ---- Selected event
                ColumnLayout {
                    Layout.preferredWidth: 280
                    Layout.maximumWidth: 280
                    Layout.fillHeight: true
                    spacing: 10

                    SectionLabel {
                        caption: page.sel >= 0 ? "Event " + (page.sel + 1) : "Selected event"
                    }
                    Text {
                        visible: page.sel < 0 && !page.readOnly
                        Layout.fillWidth: true
                        text: "Select an event in the list to edit, move or delete it."
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        wrapMode: Text.Wrap
                    }

                    Loader {
                        id: editor
                        Layout.fillWidth: true
                        readonly property var ev: page.sel >= 0 && page.sel < page.events.length ? page.events[page.sel] : null
                        active: ev !== null && !page.readOnly
                        sourceComponent: !ev ? null : ev.type === "delay" ? delayEditor : ev.type === "key" ? keyEditor : ev.type === "button" ? buttonEditor : ev.type === "scroll" ? scrollEditor : ev.type === "media" ? mediaEditor : ev.type === "power" ? powerEditor : ev.type === "cursor" ? cursorEditor : ev.type === "run" ? runEditor : null
                    }

                    RowLayout {
                        visible: page.sel >= 0 && !page.readOnly
                        spacing: 8
                        SynButton {
                            text: "▲"
                            implicitWidth: 44
                            onClicked: page.move(-1)
                        }
                        SynButton {
                            text: "▼"
                            implicitWidth: 44
                            onClicked: page.move(1)
                        }
                        SynButton {
                            text: "Remove"
                            onClicked: page.removeEvent()
                        }
                    }

                    Item {
                        Layout.fillHeight: true
                    }
                    Text {
                        Layout.fillWidth: true
                        text: page.readOnly ? "This macro is only on " + (page.deviceNames[page.devView.pid] || page.devView.pid) + ". Importing copies it to this PC; the device keeps its copy." : page.guid !== "" && page.placesOf(page.guid).length ? "On board: " + page.placesOf(page.guid).join(", ") + ". Saving updates those copies." : "Bind it to a key from a device's CUSTOMIZE tab → Macro."
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        wrapMode: Text.Wrap
                    }
                    Text {
                        visible: page.result !== ""
                        Layout.fillWidth: true
                        text: page.result
                        color: page.result.startsWith("Failed") || page.result.indexOf("failed") >= 0 ? Theme.warn : Theme.accent
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        wrapMode: Text.Wrap
                    }
                }
            }
        }
    }

    // While recording, every click and scroll in the window is captured (except on Stop).
    MouseArea {
        anchors.fill: parent
        z: 100
        visible: page.recording
        acceptedButtons: Qt.AllButtons
        hoverEnabled: true
        onPositionChanged: m => {
            if (!page.trackMoves || page.recordFilter === 1) {
                return;
            }
            const now = Date.now();
            if (now - page.lastMove < 60) {
                return;
            }
            page.lastMove = now;
            const g = mapToGlobal(m.x, m.y);
            page.record({ type: "cursor", x: Math.max(0, Math.round(g.x)), y: Math.max(0, Math.round(g.y)) });
        }
        readonly property var codes: ({ [Qt.LeftButton]: 1, [Qt.RightButton]: 2, [Qt.MiddleButton]: 3, [Qt.BackButton]: 4, [Qt.ForwardButton]: 5 })
        function onStop(m) {
            const p = mapToItem(recordButton, m.x, m.y);
            return p.x >= 0 && p.y >= 0 && p.x < recordButton.width && p.y < recordButton.height;
        }
        onPressed: m => {
            recorder.forceActiveFocus();
            if (m.button === Qt.LeftButton && onStop(m)) {
                page.stopRecording();
                return;
            }
            if (codes[m.button]) {
                page.record({ type: "button", button: codes[m.button], down: true });
            }
        }
        onReleased: m => {
            if (page.recording && codes[m.button]) {
                page.record({ type: "button", button: codes[m.button], down: false });
            }
        }
        onWheel: w => page.record({ type: "scroll", up: w.angleDelta.y > 0 })
    }

    Component {
        id: delayEditor
        RowLayout {
            spacing: 10
            Text {
                text: "Delay (ms)"
                color: Theme.text
                font.family: Theme.font
                font.pixelSize: Theme.fontSize
            }
            QQC2.SpinBox {
                from: 1
                to: 3600000
                stepSize: 10
                editable: true
                value: editor.ev ? editor.ev.ms : 50
                onValueModified: page.update(page.sel, { type: "delay", ms: value })
            }
        }
    }

    Component {
        id: keyEditor
        ColumnLayout {
            spacing: 8
            Rectangle {
                id: keyCapture
                Layout.fillWidth: true
                Layout.preferredHeight: 40
                color: Theme.bg
                border.color: activeFocus ? Theme.accent : Theme.border
                Text {
                    anchors.centerIn: parent
                    text: keyCapture.activeFocus ? "Press the new key…" : (editor.ev ? KeyNames.hidName(editor.ev.hid) + "  (click to change)" : "")
                    color: keyCapture.activeFocus ? Theme.textDim : Theme.accent
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize
                }
                TapHandler {
                    onTapped: keyCapture.forceActiveFocus()
                }
                Keys.onPressed: e => {
                    const h = KeyNames.hidOfKeyEvent(e);
                    if (h >= 0) {
                        page.update(page.sel, { type: "key", hid: h, down: editor.ev.down });
                        keyCapture.focus = false;
                    }
                    e.accepted = true;
                }
            }
            ModeTabs {
                minTabWidth: 90
                model: ["Press", "Release"]
                currentIndex: editor.ev && editor.ev.down ? 0 : 1
                onActivated: i => page.update(page.sel, { type: "key", hid: editor.ev.hid, down: i === 0 })
            }
        }
    }

    Component {
        id: buttonEditor
        ColumnLayout {
            spacing: 8
            QQC2.ComboBox {
                Layout.fillWidth: true
                model: page.buttonNames.slice(1)
                currentIndex: editor.ev ? editor.ev.button - 1 : 0
                onActivated: i => page.update(page.sel, { type: "button", button: i + 1, down: editor.ev.down })
            }
            ModeTabs {
                minTabWidth: 90
                model: ["Press", "Release"]
                currentIndex: editor.ev && editor.ev.down ? 0 : 1
                onActivated: i => page.update(page.sel, { type: "button", button: editor.ev.button, down: i === 0 })
            }
        }
    }

    Component {
        id: scrollEditor
        ModeTabs {
            minTabWidth: 90
            model: ["Up", "Down"]
            currentIndex: editor.ev && editor.ev.up ? 0 : 1
            onActivated: i => page.update(page.sel, { type: "scroll", up: i === 0 })
        }
    }

    Component {
        id: mediaEditor
        ColumnLayout {
            spacing: 8
            QQC2.ComboBox {
                objectName: "mediaKeyPick"
                Layout.fillWidth: true
                readonly property var usages: Object.keys(KeyNames.media).map(Number)
                model: usages.map(u => KeyNames.media[u])
                currentIndex: Math.max(0, usages.indexOf(editor.ev ? editor.ev.usage : 0))
                onActivated: i => page.update(page.sel, { type: "media", usage: usages[i], down: editor.ev.down })
            }
            ModeTabs {
                minTabWidth: 90
                model: ["Press", "Release"]
                currentIndex: editor.ev && editor.ev.down ? 0 : 1
                onActivated: i => page.update(page.sel, { type: "media", usage: editor.ev.usage, down: i === 0 })
            }
            Text {
                Layout.fillWidth: true
                text: i18n("Plays as a media key press through the device. Check that it does what you expect on your hardware.")
                color: Theme.textMuted
                font.family: Theme.font
                font.pixelSize: 11
                wrapMode: Text.Wrap
            }
        }
    }

    Component {
        id: powerEditor
        ColumnLayout {
            spacing: 8
            QQC2.ComboBox {
                objectName: "powerKeyPick"
                Layout.fillWidth: true
                readonly property var codes: [0x81, 0x82, 0x83]
                model: [i18n("Power"), i18n("Sleep"), i18n("Wake")]
                currentIndex: Math.max(0, codes.indexOf(editor.ev ? editor.ev.code : 0x83))
                onActivated: i => page.update(page.sel, { type: "power", code: codes[i], down: editor.ev.down })
            }
            ModeTabs {
                minTabWidth: 90
                model: ["Press", "Release"]
                currentIndex: editor.ev && editor.ev.down ? 0 : 1
                onActivated: i => page.update(page.sel, { type: "power", code: editor.ev.code, down: i === 0 })
            }
            Text {
                Layout.fillWidth: true
                text: i18n("Power and Sleep act on the whole computer when the macro plays.")
                color: Theme.warn
                font.family: Theme.font
                font.pixelSize: 11
                wrapMode: Text.Wrap
            }
        }
    }

    Component {
        id: cursorEditor
        ColumnLayout {
            spacing: 8
            RowLayout {
                spacing: 10
                Text {
                    text: "X"
                    color: Theme.text
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize
                }
                QQC2.SpinBox {
                    objectName: "cursorX"
                    from: 0
                    to: 65535
                    editable: true
                    value: editor.ev ? editor.ev.x : 0
                    onValueModified: page.update(page.sel, { type: "cursor", x: value, y: editor.ev.y })
                }
                Text {
                    text: "Y"
                    color: Theme.text
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize
                }
                QQC2.SpinBox {
                    objectName: "cursorY"
                    from: 0
                    to: 65535
                    editable: true
                    value: editor.ev ? editor.ev.y : 0
                    onValueModified: page.update(page.sel, { type: "cursor", x: editor.ev.x, y: value })
                }
            }
            Text {
                Layout.fillWidth: true
                text: i18n("Screen position in pixels from the top left. Where the pointer ends up also depends on the device's pointer settings.")
                color: Theme.textMuted
                font.family: Theme.font
                font.pixelSize: 11
                wrapMode: Text.Wrap
            }
        }
    }

    Component {
        id: runEditor
        ColumnLayout {
            spacing: 8
            QQC2.TextField {
                objectName: "runCommand"
                Layout.fillWidth: true
                text: editor.ev ? editor.ev.command : ""
                placeholderText: i18n("Command, e.g. konsole or xdg-open https://…")
                onTextEdited: page.update(page.sel, { type: "run", command: text, slot: editor.ev.slot })
            }
            Text {
                Layout.fillWidth: true
                text: i18n("The macro presses a key combination reserved for this command (F14–F24 with modifiers) and Tsuba's service runs it, so it only works while the service is running. Each different command uses one of the 46 reserved combinations.")
                color: Theme.textMuted
                font.family: Theme.font
                font.pixelSize: 11
                wrapMode: Text.Wrap
            }
        }
    }

    SynapseImportDialog {
        id: synapseDlg
        objectName: "synapseImportDialog"
        anchors.fill: parent
        z: 50
        backend: page.backend
        onImported: page.load()
    }
}
