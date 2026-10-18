import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// The macro library list: search, folders, rename / duplicate / move to a folder / delete from a
// row's menu, a marker on macros no profile uses, and a warning for bindings whose macro is gone.
ColumnLayout {
    id: ml

    required property var backend
    // The helper for tsubad's Library interface (shared with the page).
    required property var lp
    property var lib: []
    property var devs: ({})
    property var deviceNames: ({})
    // On-device macros that aren't in the library (shown below the library).
    property var deviceOnlyList: []
    // MacroUsage: {used: {guid: [..]}, missing: [..]}; unknown until the daemon answered.
    property var usage: null
    property string activeGuid: ""
    property bool editing: false
    property var devView: null

    property string search: ""
    property bool onlyUnused: false
    property var collapsed: ({})
    // The row action in progress: "", "rename", "folder", "delete" and the macro it is for.
    property string action: ""
    property var target: null
    property string message: ""

    signal openMacro(var m)
    signal openDeviceMacro(string pid, var m)
    signal importDeviceMacro(string pid, int id)
    // The daemon changed the library (rename, duplicate, folder, delete): the page reloads.
    signal changed()
    signal duplicated(string guid)
    signal removed(string guid)
    signal renamed(string guid, string name)

    spacing: 8

    function isUnused(m) {
        return usage !== null && !usage.used[m.guid];
    }
    function placesOf(g) {
        const out = [];
        for (const pid in devs) {
            if (devs[pid].macros.some(m => m.guid === g)) {
                out.push(deviceNames[pid] || pid);
            }
        }
        return out;
    }
    // Header and macro rows in display order.
    readonly property var rows: {
        const q = search.trim().toLowerCase();
        const shown = lib.filter(m => (q === "" || m.name.toLowerCase().indexOf(q) >= 0 || (m.group || "").toLowerCase().indexOf(q) >= 0) && (!onlyUnused || isUnused(m)));
        const groups = {};
        shown.forEach(m => (groups[m.group || ""] = groups[m.group || ""] || []).push(m));
        const names = Object.keys(groups).filter(g => g !== "").sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));
        const byName = (a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase());
        const out = [];
        if (groups[""]) {
            if (names.length > 0) {
                out.push({ header: "Unfiled", folder: "", count: groups[""].length });
            }
            if (!collapsed[""]) {
                groups[""].sort(byName).forEach(m => out.push({ m: m }));
            }
        }
        names.forEach(g => {
            out.push({ header: g, folder: g, count: groups[g].length });
            if (!collapsed[g]) {
                groups[g].sort(byName).forEach(m => out.push({ m: m }));
            }
        });
        return out;
    }
    readonly property int unusedCount: usage === null ? 0 : lib.filter(m => isUnused(m)).length

    function begin(kind, m) {
        action = kind;
        target = m;
        message = "";
        if (kind === "rename") {
            field.text = m.name;
        } else if (kind === "folder") {
            field.text = m.group || "";
        }
        actionPopup.open();
        if (kind !== "delete") {
            field.forceActiveFocus();
            field.selectAll();
        }
    }
    function done() {
        actionPopup.close();
        action = "";
        target = null;
    }
    function apply() {
        const m = target;
        if (!m) {
            return;
        }
        if (action === "rename") {
            const n = field.text.trim();
            if (n === "") {
                return;
            }
            if (lp.call("RenameMacro", [m.guid, n]) === null) {
                message = "Failed: " + lp.error;
                return;
            }
            renamed(m.guid, n);
        } else if (action === "folder") {
            if (lp.call("SetMacroGroup", [m.guid, field.text.trim()]) === null) {
                message = "Failed: " + lp.error;
                return;
            }
        } else if (action === "delete") {
            if (!backend.delete_macro(m.guid)) {
                message = "Failed: " + backend.status;
                return;
            }
            removed(m.guid);
        }
        done();
        changed();
    }
    function duplicate(m) {
        const g = lp.call("DuplicateMacro", [m.guid, m.name + " copy"]);
        if (g === null) {
            message = "Failed: " + lp.error;
            return;
        }
        message = "";
        changed();
        duplicated(g);
    }
    // Where a macro is used, for the delete warning: ["Gaming (key 4)", …].
    function usesOf(m) {
        return usage && usage.used[m.guid] ? usage.used[m.guid].map(u => u.profile + " (key " + u.key + ")") : [];
    }

    // ---- search + filters
    QQC2.TextField {
        id: searchField
        objectName: "macroSearch"
        Layout.fillWidth: true
        placeholderText: "Search macros"
        onTextEdited: ml.search = text
    }
    RowLayout {
        Layout.fillWidth: true
        spacing: 8
        SynCheckBox {
            objectName: "onlyUnused"
            visible: ml.usage !== null && ml.lib.length > 0
            text: "Not in use (" + ml.unusedCount + ")"
            checked: ml.onlyUnused
            onToggled: ml.onlyUnused = checked
        }
        Item {
            Layout.fillWidth: true
        }
    }

    // Bindings whose macro was deleted.
    Rectangle {
        objectName: "missingWarning"
        visible: !!ml.usage && ml.usage.missing.length > 0
        Layout.fillWidth: true
        implicitHeight: warnText.implicitHeight + 16
        color: "#22fd8611"
        border.color: Theme.warn
        Text {
            id: warnText
            anchors.fill: parent
            anchors.margins: 8
            text: ml.usage && ml.usage.missing.length > 0
                ? ml.usage.missing.length + (ml.usage.missing.length === 1 ? " key uses" : " keys use") + " a macro that no longer exists, for example key " + ml.usage.missing[0].key + " in “" + ml.usage.missing[0].profile + "”. Bind it to another macro in the device's Customize tab."
                : ""
            color: Theme.warn
            font.family: Theme.font
            font.pixelSize: 11
            wrapMode: Text.Wrap
        }
    }

    QQC2.ScrollView {
        Layout.fillWidth: true
        Layout.fillHeight: true
        contentWidth: availableWidth

        ColumnLayout {
            width: parent.width
            spacing: 6

            Repeater {
                model: ml.rows
                delegate: Item {
                    id: row
                    required property var modelData
                    readonly property bool isHeader: modelData.header !== undefined
                    readonly property var m: modelData.m
                    readonly property bool active: !isHeader && ml.editing && ml.activeGuid === m.guid
                    Layout.fillWidth: true
                    Layout.preferredHeight: isHeader ? 26 : 54

                    // Folder header
                    Rectangle {
                        visible: row.isHeader
                        anchors.fill: parent
                        color: "transparent"
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: (ml.collapsed[row.modelData.folder || ""] ? "▸ " : "▾ ") + (row.isHeader ? row.modelData.header.toUpperCase() + "  (" + row.modelData.count + ")" : "")
                            color: Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: 11
                            font.letterSpacing: 1
                        }
                        TapHandler {
                            onTapped: {
                                const c = Object.assign({}, ml.collapsed);
                                c[row.modelData.folder || ""] = !c[row.modelData.folder || ""];
                                ml.collapsed = c;
                            }
                        }
                        HoverHandler {
                            cursorShape: Qt.PointingHandCursor
                        }
                    }

                    // Macro row
                    Rectangle {
                        visible: !row.isHeader
                        objectName: row.isHeader ? "" : "macroRow_" + row.m.guid
                        anchors.fill: parent
                        color: row.active ? Theme.accentTint : (lh.hovered ? "#14ffffff" : Theme.bg)
                        border.color: row.active ? Theme.accent : Theme.borderSoft
                        Column {
                            anchors.left: parent.left
                            anchors.leftMargin: 12
                            anchors.right: tags.left
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 3
                            Text {
                                width: parent.width
                                text: row.isHeader ? "" : row.m.name
                                color: row.active ? Theme.accent : Theme.textBright
                                font.family: Theme.font
                                font.pixelSize: Theme.fontSize
                                elide: Text.ElideRight
                            }
                            Text {
                                width: parent.width
                                text: row.isHeader ? "" : row.m.events.length + " events" + (ml.placesOf(row.m.guid).length ? " · on " + ml.placesOf(row.m.guid).join(", ") : "")
                                color: Theme.textDim
                                font.family: Theme.font
                                font.pixelSize: 11
                                elide: Text.ElideRight
                            }
                        }
                        Row {
                            id: tags
                            anchors.right: parent.right
                            anchors.rightMargin: 6
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 6
                            Rectangle {
                                objectName: "unusedTag"
                                visible: !row.isHeader && ml.isUnused(row.m)
                                width: ut.implicitWidth + 10
                                height: 16
                                radius: 2
                                color: "transparent"
                                border.color: Theme.textMuted
                                anchors.verticalCenter: parent.verticalCenter
                                Text {
                                    id: ut
                                    anchors.centerIn: parent
                                    text: "NOT IN USE"
                                    color: Theme.textMuted
                                    font.family: Theme.font
                                    font.pixelSize: 9
                                    font.letterSpacing: 1
                                }
                            }
                            Rectangle {
                                objectName: row.isHeader ? "" : "rowMenu_" + row.m.guid
                                width: 28
                                height: 28
                                radius: 2
                                anchors.verticalCenter: parent.verticalCenter
                                color: mh.hovered ? "#2d2d2d" : "transparent"
                                Text {
                                    anchors.centerIn: parent
                                    text: "•••"
                                    color: Theme.text
                                    font.pixelSize: 11
                                }
                                HoverHandler {
                                    id: mh
                                    cursorShape: Qt.PointingHandCursor
                                }
                                TapHandler {
                                    onTapped: {
                                        rowMenu.macro = row.m;
                                        rowMenu.popup(parent, 0, parent.height);
                                    }
                                }
                            }
                        }
                        HoverHandler {
                            id: lh
                            cursorShape: Qt.PointingHandCursor
                        }
                        TapHandler {
                            id: rowTap
                            // The tags (and the ••• button in them) take their own taps.
                            onTapped: point => {
                                const p = tags.mapFromItem(rowTap.parent, point.position.x, point.position.y);
                                if (!tags.contains(p) || tags.children.every(c => !c.visible)) {
                                    ml.openMacro(row.m);
                                }
                            }
                        }
                    }
                }
            }
            Text {
                visible: ml.lib.length === 0
                Layout.fillWidth: true
                text: "No macros yet. Click + New to record one."
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: Theme.small
                wrapMode: Text.Wrap
            }
            Text {
                visible: ml.lib.length > 0 && ml.rows.length === 0
                Layout.fillWidth: true
                text: "No macro matches."
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: Theme.small
            }

            SectionLabel {
                visible: ml.deviceOnlyList.length > 0
                caption: "On device only"
                Layout.topMargin: 14
            }
            Repeater {
                model: ml.deviceOnlyList
                delegate: Rectangle {
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.preferredHeight: 54
                    readonly property bool active: ml.editing && ml.devView !== null && ml.devView.pid === modelData.pid && ml.devView.id === modelData.m.id
                    color: active ? Theme.accentTint : (dh.hovered ? "#14ffffff" : Theme.bg)
                    border.color: active ? Theme.accent : Theme.borderSoft
                    HoverHandler {
                        id: dh
                        cursorShape: Qt.PointingHandCursor
                    }
                    TapHandler {
                        onTapped: ml.openDeviceMacro(modelData.pid, modelData.m)
                    }
                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 12
                        anchors.rightMargin: 8
                        Column {
                            Layout.fillWidth: true
                            spacing: 3
                            Text {
                                width: parent.width
                                text: modelData.m.name
                                color: Theme.text
                                font.family: Theme.font
                                font.pixelSize: Theme.fontSize
                                elide: Text.ElideRight
                            }
                            Text {
                                width: parent.width
                                text: (ml.deviceNames[modelData.pid] || modelData.pid) + " · " + modelData.m.events.length + " events"
                                color: Theme.textDim
                                font.family: Theme.font
                                font.pixelSize: 11
                                elide: Text.ElideRight
                            }
                        }
                        SynButton {
                            objectName: "importRowButton"
                            text: "Import"
                            implicitWidth: 72
                            onClicked: ml.importDeviceMacro(modelData.pid, modelData.m.id)
                        }
                    }
                }
            }
        }
    }

    Text {
        objectName: "listMessage"
        visible: ml.message !== ""
        Layout.fillWidth: true
        text: ml.message
        color: Theme.warn
        font.family: Theme.font
        font.pixelSize: Theme.small
        wrapMode: Text.Wrap
    }

    // ---- the row menu and its dialogs
    QQC2.Menu {
        id: rowMenu
        property var macro: null
        QQC2.MenuItem {
            objectName: "menuRename"
            text: "Rename"
            enabled: !ml.backend.readonly
            onTriggered: ml.begin("rename", rowMenu.macro)
        }
        QQC2.MenuItem {
            objectName: "menuDuplicate"
            text: "Duplicate"
            enabled: !ml.backend.readonly
            onTriggered: ml.duplicate(rowMenu.macro)
        }
        QQC2.MenuItem {
            objectName: "menuFolder"
            text: "Move to folder…"
            enabled: !ml.backend.readonly
            onTriggered: ml.begin("folder", rowMenu.macro)
        }
        QQC2.MenuItem {
            objectName: "menuDelete"
            text: "Delete"
            enabled: !ml.backend.readonly
            onTriggered: ml.begin("delete", rowMenu.macro)
        }
    }

    QQC2.Popup {
        id: actionPopup
        objectName: "macroActionPopup"
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        width: 360
        padding: 16
        modal: true
        onClosed: {
            ml.action = "";
            ml.target = null;
        }
        background: Rectangle {
            color: Theme.card
            border.color: Theme.accent
        }
        contentItem: ColumnLayout {
            spacing: 10
            Text {
                Layout.fillWidth: true
                text: ml.action === "rename" ? "Rename macro" : (ml.action === "folder" ? "Folder" : (ml.target ? "Delete “" + ml.target.name + "”?" : ""))
                color: Theme.textBright
                font.family: Theme.font
                font.pixelSize: Theme.fontSize
                font.bold: true
                wrapMode: Text.Wrap
            }
            QQC2.TextField {
                id: field
                objectName: "macroActionField"
                visible: ml.action === "rename" || ml.action === "folder"
                Layout.fillWidth: true
                placeholderText: ml.action === "folder" ? "Folder name (empty = none)" : "Macro name"
                onAccepted: ml.apply()
            }
            Flow {
                visible: ml.action === "folder"
                Layout.fillWidth: true
                spacing: 6
                Repeater {
                    model: ml.action === "folder" ? [...new Set(ml.lib.map(m => m.group).filter(g => g))].sort() : []
                    delegate: Rectangle {
                        required property string modelData
                        width: ct.implicitWidth + 16
                        height: 24
                        radius: 12
                        color: "transparent"
                        border.color: Theme.border
                        Text {
                            id: ct
                            anchors.centerIn: parent
                            text: modelData
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: 11
                        }
                        TapHandler {
                            onTapped: field.text = modelData
                        }
                    }
                }
            }
            Text {
                objectName: "deleteWarning"
                visible: ml.action === "delete"
                Layout.fillWidth: true
                text: ml.target ? (ml.usesOf(ml.target).length > 0
                    ? "It is used by " + ml.usesOf(ml.target).join(", ") + ". Those keys lose it. This also removes it from your devices."
                    : "This removes it from the library and from your devices.") : ""
                color: Theme.text
                font.family: Theme.font
                font.pixelSize: Theme.small
                wrapMode: Text.Wrap
            }
            Text {
                visible: ml.message !== "" && actionPopup.opened
                Layout.fillWidth: true
                text: ml.message
                color: Theme.warn
                font.family: Theme.font
                font.pixelSize: Theme.small
                wrapMode: Text.Wrap
            }
            RowLayout {
                spacing: 8
                Item {
                    Layout.fillWidth: true
                }
                SynButton {
                    text: "Cancel"
                    implicitWidth: 80
                    onClicked: ml.done()
                }
                SynButton {
                    objectName: "macroActionOk"
                    text: ml.action === "delete" ? "Delete" : "Save"
                    primary: true
                    implicitWidth: 80
                    onClicked: ml.apply()
                }
            }
        }
    }
}
