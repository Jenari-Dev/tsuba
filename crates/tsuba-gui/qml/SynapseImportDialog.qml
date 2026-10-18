import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Dialogs

// "Import from Synapse": reads the profiles, macros and Lighting Studio effects that Synapse 4
// keeps on a Windows install (dual boot) and writes the chosen ones through tsubad. A full-page
// overlay like GameScanDialog. Synapse's own files are only read.
Item {
    id: dlg

    property var backend
    // Section to show first: "device:<pid>", "macros" or "lighting".
    property string section: ""
    property var stores: []
    property string storePath: ""
    property var view: null
    property string phase: "idle"           // idle | scanning | ready | error | importing | done
    property string errorText: ""
    property var picked: ({})               // "p:<pid>:<guid>" | "m:<guid>" | "l:<id>" -> true
    property var deviceSettings: ({})       // pid -> profile guid (device-wide settings)
    property var pin: ({})                  // pid -> true: also pin imported profiles to free on-board slots
    property var expanded: ({})
    property var progressLines: []
    property var result: null

    signal closed
    signal imported

    visible: false

    function open(sec) {
        section = sec || "";
        picked = ({});
        deviceSettings = ({});
        pin = ({});
        expanded = ({});
        result = null;
        progressLines = [];
        stores = JSON.parse(backend.synapse_stores() || "[]");
        const forced = backend.env_var("TSUBA_SYNAPSE");
        storePath = forced !== "" ? forced : (stores.length > 0 ? stores[0] : "");
        visible = true;
        forceActiveFocus();
        if (storePath !== "") {
            scan();
        } else {
            phase = "idle";
        }
    }
    function close() {
        visible = false;
        closed();
    }
    function scan() {
        phase = "scanning";
        view = null;
        backend.synapse_scan(storePath);
    }
    function key(kind, a, b) {
        return kind + ":" + a + (b === undefined ? "" : ":" + b);
    }
    function setPicked(k, on) {
        const p = Object.assign({}, picked);
        if (on) {
            p[k] = true;
        } else {
            delete p[k];
        }
        picked = p;
    }
    function toggleExpanded(k) {
        const e = Object.assign({}, expanded);
        e[k] = !e[k];
        expanded = e;
    }
    function pickedOn(d) {
        return d.profiles.filter(p => picked[key("p", d.pid, p.guid)]).length;
    }
    // Profiles go into the PC profile library: no on-board slot limit, and the device needn't
    // be connected (pinning and device-wide settings need it connected and writable).
    function canImport(d) {
        return d.importable && !!d.target && !!view && view.connected;
    }
    function canPin(d) {
        return d.importable && d.attached && d.writable && d.free_pins > 0;
    }
    function setPin(pid, on) {
        const p = Object.assign({}, pin);
        if (on) {
            p[pid] = true;
        } else {
            delete p[pid];
        }
        pin = p;
    }
    // The device whose page opened the dialog first, then connected ones, then the rest.
    readonly property var devices: {
        if (!view) {
            return [];
        }
        const focus = section.startsWith("device:") ? section.slice(7) : "";
        const rank = d => (d.pid === focus || d.target === focus ? 0 : 3) + (d.attached ? 0 : 1) + (d.importable ? 0 : 1);
        return view.devices.slice().sort((a, b) => rank(a) - rank(b));
    }
    // Macros the chosen profiles need (imported with them).
    readonly property var neededMacros: {
        const s = {};
        if (view) {
            view.devices.forEach(d => d.profiles.forEach(p => {
                if (picked[key("p", d.pid, p.guid)]) {
                    p.macros.forEach(m => s[m] = true);
                }
            }));
        }
        return s;
    }
    readonly property int pickedCount: Object.keys(picked).length + Object.keys(deviceSettings).length
    function selectionJson() {
        const sel = { profiles: {}, device_settings: deviceSettings, pin: pin, macros: [], lighting: [] };
        Object.keys(picked).forEach(k => {
            const parts = k.split(":");
            if (parts[0] === "p") {
                (sel.profiles[parts[1]] = sel.profiles[parts[1]] || []).push(parts.slice(2).join(":"));
            } else if (parts[0] === "m") {
                sel.macros.push(parts[1]);
            } else if (parts[0] === "l") {
                sel.lighting.push(parts.slice(1).join(":"));
            }
        });
        return JSON.stringify(sel);
    }
    function startImport() {
        phase = "importing";
        progressLines = [];
        backend.synapse_import(view.store, selectionJson());
    }
    function summaryText() {
        let profiles = 0, macros = 0, effects = 0;
        Object.keys(picked).forEach(k => {
            if (k.startsWith("p:")) profiles++;
            else if (k.startsWith("m:")) macros++;
            else if (k.startsWith("l:")) effects++;
        });
        const extra = Object.keys(neededMacros).filter(g => !picked["m:" + g]).length;
        const parts = [];
        if (profiles) parts.push(i18np("%1 profile", "%1 profiles", profiles));
        if (macros + extra) parts.push(i18np("%1 macro", "%1 macros", macros + extra));
        if (effects) parts.push(i18np("%1 effect", "%1 effects", effects));
        const nd = Object.keys(deviceSettings).length;
        if (nd) parts.push(i18np("device-wide settings for %1 device", "device-wide settings for %1 devices", nd));
        return parts.length ? i18nc("%1 is a list such as “2 profiles, 1 macro”", "%1 selected", parts.join(i18nc("list separator", ", "))) : i18n("Pick what to bring over. Profiles go into each device's profile library (as many as you like); macros and effects are saved on this PC.");
    }
    function scrollToSection() {
        let target = null;
        if (section === "macros") {
            target = macrosHeader;
        } else if (section === "lighting") {
            target = lightingHeader;
        }
        if (target) {
            flick.contentY = Math.max(0, Math.min(target.y, flick.contentHeight - flick.height));
        }
    }

    Connections {
        target: dlg.backend
        ignoreUnknownSignals: true
        function onSynapse_scanned(json) {
            if (!dlg.visible) {
                return;
            }
            const v = JSON.parse(json);
            if (v.error) {
                dlg.errorText = v.error;
                dlg.phase = "error";
                return;
            }
            dlg.view = v;
            dlg.phase = "ready";
            // Headless UI check: TSUBA_SYNAPSE_PICK=1 picks the first importable profile per device
            // (and opens its details), every macro and effect.
            if (dlg.backend.env_var("TSUBA_SYNAPSE_PICK") === "1") {
                const p = {};
                const e = {};
                v.devices.forEach(d => {
                    if (d.profiles.length > 0) {
                        e[dlg.key("p", d.pid, d.profiles[0].guid)] = true;
                        if (d.importable && d.attached) {
                            p[dlg.key("p", d.pid, d.profiles[0].guid)] = true;
                        }
                    }
                });
                v.macros.forEach(m => p[dlg.key("m", m.guid)] = true);
                v.lighting.forEach(l => {
                    if (l.importable) {
                        p[dlg.key("l", l.id)] = true;
                    }
                });
                dlg.picked = p;
                dlg.expanded = e;
            }
            scrollTimer.restart();
        }
        function onSynapse_progress(text) {
            if (dlg.visible) {
                dlg.progressLines = dlg.progressLines.concat([text]).slice(-200);
            }
        }
        function onSynapse_imported(json) {
            if (!dlg.visible) {
                return;
            }
            dlg.result = JSON.parse(json);
            dlg.phase = "done";
            dlg.imported();
        }
    }

    // Scroll once the lists have been laid out.
    Timer {
        id: scrollTimer
        interval: 150
        onTriggered: dlg.scrollToSection()
    }

    FolderDialog {
        id: folderPick
        title: i18n("Choose the Windows drive or user folder")
        onAccepted: {
            let p = selectedFolder.toString();
            p = decodeURIComponent(p.replace(/^file:\/\//, ""));
            dlg.storePath = p;
            dlg.scan();
        }
    }

    // Dim the page below and swallow its input.
    Rectangle {
        anchors.fill: parent
        color: Theme.chrome
        opacity: 0.7
    }
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        onWheel: w => w.accepted = true
    }
    Keys.onEscapePressed: if (dlg.phase !== "importing") dlg.close()

    component Badge: Rectangle {
        property alias text: bt.text
        property color tone: Theme.textDim
        implicitWidth: bt.implicitWidth + 12
        implicitHeight: 18
        radius: 2
        color: "transparent"
        border.color: tone
        Text {
            id: bt
            anchors.centerIn: parent
            color: parent.tone
            font.family: Theme.font
            font.pixelSize: 10
            font.letterSpacing: 1
        }
    }

    // One importable item: checkbox, name, tags, and expandable details.
    component ItemRow: ColumnLayout {
        id: row
        property string itemKey: ""
        objectName: itemKey
        property string title: ""
        property string detail: ""
        property var tags: []            // [[text, colour]]
        property var lines: []           // what will be written
        property var deviceLines: []     // device-wide
        property var warnings: []
        property bool checkable: true
        property bool forcedOn: false
        property string forcedNote: ""
        Layout.fillWidth: true
        spacing: 0
        readonly property bool open: !!dlg.expanded[itemKey]
        readonly property bool on: forcedOn || !!dlg.picked[itemKey]
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 40
            color: row.on ? Theme.accentTint : (rh.hovered ? Theme.panelRaised : "transparent")
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 10
                anchors.rightMargin: 10
                spacing: 10
                SynCheckBox {
                    enabled: row.checkable && !row.forcedOn
                    checked: row.on
                    focusPolicy: Qt.NoFocus
                    onToggled: dlg.setPicked(row.itemKey, checked)
                }
                Text {
                    textFormat: Text.PlainText
                    text: row.title
                    color: row.checkable ? Theme.textBright : Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize
                    elide: Text.ElideRight
                    Layout.maximumWidth: 320
                }
                Repeater {
                    model: row.tags
                    delegate: Badge {
                        required property var modelData
                        text: modelData[0]
                        tone: modelData[1]
                    }
                }
                Text {
                    Layout.fillWidth: true
                    textFormat: Text.PlainText
                    text: row.forcedOn ? row.forcedNote : row.detail
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    elide: Text.ElideRight
                }
                Text {
                    visible: row.warnings.length > 0
                    text: "⚠ " + row.warnings.length
                    color: Theme.warn
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                }
                Text {
                    visible: row.lines.length + row.deviceLines.length + row.warnings.length > 0
                    text: row.open ? "▴" : "▾"
                    color: Theme.text
                    font.pixelSize: 12
                }
            }
            HoverHandler {
                id: rh
                cursorShape: Qt.PointingHandCursor
            }
            TapHandler {
                onTapped: dlg.toggleExpanded(row.itemKey)
            }
        }
        ColumnLayout {
            visible: row.open
            Layout.fillWidth: true
            Layout.leftMargin: 48
            Layout.rightMargin: 12
            Layout.topMargin: 4
            Layout.bottomMargin: 8
            spacing: 3
            Repeater {
                model: row.lines
                delegate: Text {
                    required property string modelData
                    Layout.fillWidth: true
                    textFormat: Text.PlainText
                    text: "✓  " + modelData
                    color: Theme.text
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    wrapMode: Text.Wrap
                }
            }
            Repeater {
                model: row.deviceLines
                delegate: Text {
                    required property string modelData
                    Layout.fillWidth: true
                    textFormat: Text.PlainText
                    text: i18n("◇  %1  (device-wide)", modelData)
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    wrapMode: Text.Wrap
                }
            }
            Repeater {
                model: row.warnings
                delegate: Text {
                    required property string modelData
                    Layout.fillWidth: true
                    textFormat: Text.PlainText
                    text: "⚠  " + modelData
                    color: Theme.warn
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    wrapMode: Text.Wrap
                }
            }
        }
    }

    Rectangle {
        anchors.centerIn: parent
        width: Math.min(1040, dlg.width - 60)
        height: Math.min(760, dlg.height - 40)
        color: Theme.panel
        border.color: Theme.borderSoft
        radius: 5

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: 18
            spacing: 12

            RowLayout {
                Layout.fillWidth: true
                spacing: 12
                Text {
                    text: i18n("IMPORT FROM SYNAPSE")
                    color: Theme.accent
                    font.family: Theme.titleFont
                    font.pixelSize: 16
                }
                Text {
                    Layout.fillWidth: true
                    text: i18n("Profiles, macros and Lighting Studio effects from Synapse 4 on a Windows install. Its files are only read.")
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    elide: Text.ElideRight
                }
            }

            // Where the Synapse data is.
            RowLayout {
                Layout.fillWidth: true
                spacing: 10
                enabled: dlg.phase !== "importing"
                SectionLabel {
                    caption: i18n("Synapse data")
                }
                SynDropdown {
                    id: storePick
                    objectName: "synapseStore"
                    Layout.fillWidth: true
                    visible: dlg.stores.length > 0 || dlg.storePath !== ""
                    model: dlg.stores.indexOf(dlg.storePath) >= 0 || dlg.storePath === "" ? dlg.stores : dlg.stores.concat([dlg.storePath])
                    currentIndex: Math.max(0, model.indexOf(dlg.storePath))
                    displayText: dlg.storePath.replace(/\/AppData\/Local\/Razer\/RazerAppEngine\/User Data\/Default\/IndexedDB\/https_apps\.razer\.com_0\.indexeddb\.leveldb$/, "")
                    onActivated: i => {
                        dlg.storePath = model[i];
                        dlg.scan();
                    }
                }
                Text {
                    visible: dlg.stores.length === 0 && dlg.storePath === ""
                    Layout.fillWidth: true
                    text: i18n("No mounted Windows drive with Synapse data was found. Mount the drive in your file manager, or choose its folder.")
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    wrapMode: Text.Wrap
                }
                SynButton {
                    text: i18n("Choose folder…")
                    onClicked: folderPick.open()
                }
                SynButton {
                    text: i18n("Rescan")
                    visible: dlg.storePath !== ""
                    enabled: dlg.phase !== "scanning"
                    onClicked: {
                        dlg.stores = JSON.parse(dlg.backend.synapse_stores() || "[]");
                        dlg.scan();
                    }
                }
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 1
                color: Theme.borderSoft
            }

            // Status for the non-list phases.
            Text {
                visible: dlg.phase === "scanning" || dlg.phase === "error" || dlg.phase === "idle"
                Layout.fillWidth: true
                Layout.fillHeight: true
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: dlg.phase === "scanning" ? i18n("Reading the Synapse data…") : (dlg.phase === "error" ? dlg.errorText : i18n("Choose where the Synapse data is."))
                color: dlg.phase === "error" ? Theme.warn : Theme.textDim
                font.family: Theme.font
                font.pixelSize: Theme.fontSize
            }

            // The list.
            Flickable {
                id: flick
                objectName: "synapseList"
                visible: dlg.phase === "ready"
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                contentWidth: width
                contentHeight: content.implicitHeight
                boundsBehavior: Flickable.StopAtBounds
                QQC2.ScrollBar.vertical: QQC2.ScrollBar {}

                ColumnLayout {
                    id: content
                    width: flick.width - 14
                    spacing: 6

                    SectionLabel {
                        caption: i18n("Device profiles")
                    }
                    Text {
                        visible: !!dlg.view && dlg.view.devices.length === 0
                        text: i18n("Synapse has no device profiles here.")
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                    }
                    Repeater {
                        model: dlg.devices
                        delegate: ColumnLayout {
                            id: dev
                            required property var modelData
                            readonly property int chosen: dlg.pickedOn(modelData)
                            Layout.fillWidth: true
                            Layout.bottomMargin: 8
                            spacing: 0
                            Rectangle {
                                Layout.fillWidth: true
                                Layout.preferredHeight: 44
                                color: Theme.panelRaised
                                RowLayout {
                                    anchors.fill: parent
                                    anchors.leftMargin: 12
                                    anchors.rightMargin: 10
                                    spacing: 10
                                    Text {
                                        textFormat: Text.PlainText
                                        text: dev.modelData.name
                                        color: Theme.textBright
                                        font.family: Theme.font
                                        font.pixelSize: Theme.fontSize
                                        font.bold: true
                                    }
                                    Badge {
                                        visible: dlg.canImport(dev.modelData) && dev.chosen > 0
                                        text: i18n("%1 PICKED", dev.chosen)
                                        tone: Theme.accent
                                    }
                                    Text {
                                        Layout.fillWidth: true
                                        textFormat: Text.PlainText
                                        text: !dev.modelData.importable ? dev.modelData.note
                                            : !dlg.canImport(dev.modelData) ? i18n("Tsuba isn't running, so nothing can be imported.")
                                            : dev.modelData.note !== "" ? dev.modelData.note
                                            : (dev.modelData.attached
                                                ? i18np("%1 profile in Synapse → profile library", "%1 profiles in Synapse → profile library", dev.modelData.profiles.length)
                                                : i18np("%1 profile in Synapse → profile library (not connected)", "%1 profiles in Synapse → profile library (not connected)", dev.modelData.profiles.length))
                                        color: Theme.textDim
                                        font.family: Theme.font
                                        font.pixelSize: Theme.small
                                        elide: Text.ElideRight
                                    }
                                    SynCheckBox {
                                        objectName: "pin:" + dev.modelData.pid
                                        visible: dev.modelData.importable && dev.modelData.attached
                                        enabled: dlg.canPin(dev.modelData)
                                        checked: !!dlg.pin[dev.modelData.pid]
                                        focusPolicy: Qt.NoFocus
                                        onToggled: dlg.setPin(dev.modelData.pid, checked)
                                        QQC2.ToolTip.visible: hovered
                                        QQC2.ToolTip.text: i18n("Also write the picked profiles into the device's free on-board slots, so its profile button reaches them without Tsuba.")
                                    }
                                    Text {
                                        visible: dev.modelData.importable && dev.modelData.attached
                                        text: !dev.modelData.writable ? i18n("Pinning needs write access")
                                            : dev.modelData.free_pins > 0 ? i18np("Also pin to device (%2 of %1 free slot)", "Also pin to device (%2 of %1 free slots)", dev.modelData.free_pins, Math.min(dev.modelData.free_pins, Math.max(dev.chosen, 1)))
                                            : i18n("No free on-board slot to pin to")
                                        color: dlg.canPin(dev.modelData) ? Theme.text : Theme.textDim
                                        font.family: Theme.font
                                        font.pixelSize: Theme.small
                                    }
                                    Text {
                                        visible: dev.modelData.importable && dev.modelData.attached
                                        text: i18n("Device-wide settings")
                                        color: Theme.textDim
                                        font.family: Theme.font
                                        font.pixelSize: Theme.small
                                    }
                                    SynDropdown {
                                        visible: dev.modelData.importable && dev.modelData.attached
                                        enabled: dlg.canImport(dev.modelData) && dev.modelData.writable
                                        implicitWidth: 220
                                        model: [i18n("Keep Tsuba's")].concat(dev.modelData.profiles.map(p => i18n("From “%1”", p.name)))
                                        currentIndex: {
                                            const g = dlg.deviceSettings[dev.modelData.pid];
                                            return g ? 1 + dev.modelData.profiles.findIndex(p => p.guid === g) : 0;
                                        }
                                        onActivated: i => {
                                            const s = Object.assign({}, dlg.deviceSettings);
                                            if (i === 0) {
                                                delete s[dev.modelData.pid];
                                            } else {
                                                s[dev.modelData.pid] = dev.modelData.profiles[i - 1].guid;
                                            }
                                            dlg.deviceSettings = s;
                                        }
                                        QQC2.ToolTip.visible: hovered
                                        QQC2.ToolTip.text: i18n("Polling rate, lighting and the command dial are one setting per device in Tsuba; pick the profile to take them from.")
                                    }
                                }
                            }
                            Repeater {
                                model: dev.modelData.profiles
                                delegate: ItemRow {
                                    required property var modelData
                                    itemKey: dlg.key("p", dev.modelData.pid, modelData.guid)
                                    title: modelData.name
                                    tags: (modelData.active ? [[i18n("ACTIVE IN SYNAPSE"), Theme.accent]] : []).concat(modelData.in_library ? [[i18n("IN LIBRARY"), Theme.textDim]] : [])
                                    detail: {
                                        const n = modelData.summary.length, w = modelData.device_wide.length;
                                        if (w) {
                                            return modelData.in_library
                                                ? i18np("%1 setting + %2 device-wide · updates the library copy", "%1 settings + %2 device-wide · updates the library copy", n, w)
                                                : i18np("%1 setting + %2 device-wide", "%1 settings + %2 device-wide", n, w);
                                        }
                                        return modelData.in_library
                                            ? i18np("%1 setting · updates the library copy", "%1 settings · updates the library copy", n)
                                            : i18np("%1 setting", "%1 settings", n);
                                    }
                                    lines: modelData.summary
                                    deviceLines: modelData.device_wide
                                    warnings: modelData.warnings
                                    checkable: dlg.canImport(dev.modelData)
                                }
                            }
                        }
                    }

                    SectionLabel {
                        id: macrosHeader
                        Layout.topMargin: 8
                        caption: i18n("Macros")
                    }
                    Text {
                        visible: !!dlg.view && dlg.view.macros.length === 0
                        text: i18n("Synapse has no macros here.")
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                    }
                    Repeater {
                        model: dlg.view ? dlg.view.macros : []
                        delegate: ItemRow {
                            required property var modelData
                            itemKey: dlg.key("m", modelData.guid)
                            title: modelData.name
                            tags: modelData.in_library ? [[i18n("IN LIBRARY"), Theme.textDim]] : []
                            detail: modelData.in_library
                                ? i18np("%1 event · replaces the library copy", "%1 events · replaces the library copy", modelData.events)
                                : i18np("%1 event", "%1 events", modelData.events)
                            warnings: modelData.warnings
                            forcedOn: !!dlg.neededMacros[modelData.guid]
                            forcedNote: i18n("Imported with a selected profile that uses it")
                        }
                    }

                    SectionLabel {
                        id: lightingHeader
                        Layout.topMargin: 8
                        caption: i18n("Lighting Studio effects")
                    }
                    Text {
                        visible: !!dlg.view && dlg.view.lighting.length === 0
                        text: i18n("Synapse has no Chroma Studio effects here.")
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                    }
                    Repeater {
                        model: dlg.view ? dlg.view.lighting : []
                        delegate: ItemRow {
                            required property var modelData
                            itemKey: dlg.key("l", modelData.id)
                            title: modelData.name
                            tags: modelData.exists ? [[i18n("NAME IN USE"), Theme.textDim]] : []
                            detail: !modelData.importable ? i18n("Nothing in it maps to a Tsuba device")
                                : modelData.exists ? i18np("%1 layer on %2 · saved under a new name", "%1 layers on %2 · saved under a new name", modelData.layers, modelData.devices.join(i18nc("list separator", ", ")))
                                : i18np("%1 layer on %2", "%1 layers on %2", modelData.layers, modelData.devices.join(i18nc("list separator", ", ")))
                            warnings: modelData.warnings
                            checkable: modelData.importable
                        }
                    }
                    Text {
                        visible: !!dlg.view && dlg.view.errors.length > 0
                        Layout.fillWidth: true
                        Layout.topMargin: 8
                        text: dlg.view ? i18np("%1 Synapse record couldn't be read and was left out.", "%1 Synapse records couldn't be read and were left out.", dlg.view.errors.length) : ""
                        color: Theme.warn
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        wrapMode: Text.Wrap
                    }
                }
            }

            // Progress / result.
            Flickable {
                visible: dlg.phase === "importing" || dlg.phase === "done"
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                contentHeight: log.implicitHeight
                boundsBehavior: Flickable.StopAtBounds
                QQC2.ScrollBar.vertical: QQC2.ScrollBar {}
                ColumnLayout {
                    id: log
                    width: parent.width - 14
                    spacing: 4
                    SectionLabel {
                        caption: dlg.phase === "importing" ? i18n("Importing…") : i18n("Import finished")
                    }
                    Text {
                        visible: dlg.phase === "importing"
                        Layout.fillWidth: true
                        text: dlg.progressLines.length ? dlg.progressLines[dlg.progressLines.length - 1] : i18n("Starting…")
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.fontSize
                        wrapMode: Text.Wrap
                    }
                    Repeater {
                        model: dlg.result ? dlg.result.done : []
                        delegate: Text {
                            required property string modelData
                            Layout.fillWidth: true
                            textFormat: Text.PlainText
                            text: "✓  " + modelData
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                            wrapMode: Text.Wrap
                        }
                    }
                    Repeater {
                        model: dlg.result ? dlg.result.failed : []
                        delegate: Text {
                            required property string modelData
                            Layout.fillWidth: true
                            textFormat: Text.PlainText
                            text: "⚠  " + modelData
                            color: Theme.warn
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                            wrapMode: Text.Wrap
                        }
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 10
                Text {
                    Layout.fillWidth: true
                    text: dlg.phase === "done" ? (dlg.result.failed.length ? i18np("%1 item didn't import; see above.", "%1 items didn't import; see above.", dlg.result.failed.length) : i18n("Everything was imported."))
                        : dlg.backend.readonly && dlg.phase === "ready" ? i18n("Tsuba is running read-only, so nothing can be imported.")
                        : dlg.phase === "ready" ? dlg.summaryText() : ""
                    color: dlg.phase === "ready" && dlg.pickedCount > 0 && !dlg.backend.readonly ? Theme.accent : Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    elide: Text.ElideRight
                }
                SynButton {
                    objectName: "synapseClose"
                    text: dlg.phase === "done" ? i18nc("button: finish", "Done") : i18nc("button: close dialog", "Close")
                    enabled: dlg.phase !== "importing"
                    Layout.minimumWidth: 90
                    onClicked: dlg.close()
                }
                SynButton {
                    objectName: "synapseImport"
                    visible: dlg.phase !== "done"
                    text: i18n("Import selected")

                    primary: true
                    Layout.minimumWidth: 150
                    enabled: dlg.phase === "ready" && dlg.pickedCount > 0 && !dlg.backend.readonly
                    onClicked: dlg.startImport()
                }
            }
        }
    }
}
