import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import QtQuick.Layouts

// One device: header, CUSTOMIZE / PERFORMANCE / LIGHTING tabs (Synapse layout).
Rectangle {
    id: page

    required property var backend
    required property string pid
    property int tabIndex: 0
    property var st: ({})
    // The PC profile library (any number of profiles). The active one runs in an on-board
    // slot; `profile` is that slot, which the Customize / Performance tabs edit (edits are saved
    // into the active library profile by tsubad).
    property var lib: ({ profiles: [], active: null, active_slot: 0, max_slots: 0, free_slots: [], slots: [] })
    property int profile: 1
    // Bumped when the tabs must re-read the slot (another profile was written into it).
    property int libRev: 0
    property string profileStatus: ""
    // Library profile being activated ("" when idle).
    property string switching: ""
    // Profile to delete once the switch away from it finished.
    property string pendingDelete: ""
    readonly property var current: (lib.profiles || []).find(p => p.id === lib.active) || null
    readonly property bool hasProfiles: (lib.profiles || []).length > 0 || (lib.slots || []).length > 0

    function profileById(id) {
        return (lib.profiles || []).find(p => p.id === id) || null;
    }

    // The helper for the Library interface (import / export, factory template, lighting).
    property alias lp: provider
    LibProvider {
        id: provider
    }
    // Linked games per library profile id: names of the apps that switch to it on this device.
    property var linkedApps: ({})
    function loadLinks() {
        const s = backend.app_links ? backend.app_links() : "";
        const map = {};
        if (s !== "") {
            JSON.parse(s).links.forEach(l => {
                const ref = l.device_profiles ? l.device_profiles[pid] : undefined;
                if (typeof ref === "string") {
                    (map[ref] = map[ref] || []).push(l.name || l.matches);
                }
            });
        }
        linkedApps = map;
    }
    // Profile file names: "<name>.tsuba-profile.json".
    function fileNameFor(p) {
        return (p.name.replace(/[^\w .-]+/g, "_").trim() || "profile") + ".tsuba-profile.json";
    }
    function importFiles(urls) {
        let ok = 0;
        const failed = [];
        urls.forEach(u => {
            const path = lp.pathOfUrl(u);
            if (lp.call("Tsuba1.ImportLibraryProfile", [pid, path]) !== null) {
                ok++;
            } else {
                failed.push(path.split("/").pop() + ": " + lp.error);
            }
        });
        profileStatus = (ok > 0 ? i18np("Imported %1 profile", "Imported %1 profiles", ok) : "") + (failed.length ? (ok > 0 ? "; " : "Failed: ") + failed.join("; ") : "");
        loadProfiles();
        Theme.profilesRev++;
    }
    function exportOne(url) {
        const p = current;
        const path = lp.pathOfUrl(url);
        profileStatus = lp.call("Tsuba1.ExportLibraryProfile", [pid, p.id, path]) !== null ? i18n("Exported “%1”", p.name) : "Failed: " + lp.error;
    }
    function exportMany(url) {
        const ids = Object.keys(exportSel).filter(k => exportSel[k]);
        const r = lp.json("ExportLibraryProfiles", [pid, JSON.stringify(ids), lp.pathOfUrl(url)], null);
        profileStatus = r ? i18np("Exported %1 profile", "Exported %1 profiles", r.length) : "Failed: " + lp.error;
    }
    // Profiles ticked in the "Export…" list.
    property var exportSel: ({})
    function selectAllForExport(on) {
        const m = {};
        (lib.profiles || []).forEach(p => m[p.id] = on);
        exportSel = m;
    }
    readonly property int exportCount: Object.keys(exportSel).filter(k => exportSel[k]).length
    // Lighting that follows the profile.
    function setProfileLighting(on) {
        const p = current;
        if (!p) {
            return;
        }
        const ok = lp.call("SetProfileLighting", [pid, p.id, on ? "current" : "off"]) !== null;
        profileAction(ok, on ? i18n("“%1” now keeps its own lighting", p.name) : i18n("“%1” no longer changes the lighting", p.name));
        if (!ok) {
            profileStatus = "Failed: " + lp.error;
        }
    }

    FileDialog {
        id: importDlg
        title: i18n("Import profiles")
        fileMode: FileDialog.OpenFiles
        nameFilters: [i18n("Tsuba profiles (*.tsuba-profile.json *.json)"), i18n("All files (*)")]
        onAccepted: page.importFiles(selectedFiles)
    }
    FileDialog {
        id: exportDlg
        title: i18n("Export profile")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "json"
        nameFilters: [i18n("Tsuba profiles (*.tsuba-profile.json)"), i18n("All files (*)")]
        onAccepted: page.exportOne(selectedFile)
    }
    FolderDialog {
        id: exportDirDlg
        title: i18n("Export the chosen profiles to a folder")
        onAccepted: page.exportMany(selectedFolder)
    }

    function currentName() {
        if (current) {
            return current.name;
        }
        const s = (lib.slots || []).find(x => x.slot === lib.active_slot);
        return s ? (s.name !== "" ? s.name : "On-board profile " + s.slot) : "";
    }

    // Re-read the library; returns true when the active profile changed.
    function loadProfiles() {
        const s = backend.library_profiles(pid);
        if (s === "") {
            return false;
        }
        const before = lib.active;
        const beforeSlot = lib.active_slot;
        lib = JSON.parse(s);
        loadLinks();
        if (lib.active_slot) {
            profile = lib.active_slot;
        }
        return before !== lib.active || beforeSlot !== lib.active_slot;
    }

    function activate(id) {
        if (id === lib.active || writesBlocked || switching !== "") {
            return;
        }
        switching = id;
        profileStatus = "";
        backend.activate_library_profile(pid, id);
    }

    function profileAction(ok, done) {
        profileStatus = ok ? done : "Failed: " + backend.status;
        loadProfiles();
        Theme.profilesRev++;
    }

    // Where to go before deleting the active profile: a pinned one (usually just a switch).
    function fallbackFor(id) {
        const others = (lib.profiles || []).filter(p => p.id !== id);
        return others.find(p => p.pinned_slot !== null && p.pinned_slot !== undefined) || others[0] || null;
    }

    function deleteProfile(id) {
        const p = profileById(id);
        if (!p) {
            return;
        }
        if (id === lib.active) {
            const to = fallbackFor(id);
            if (!to) {
                return;
            }
            pendingDelete = id;
            activate(to.id);
            return;
        }
        profileAction(backend.delete_library_profile(pid, id), "Deleted “" + p.name + "”");
    }

    // What the database says about this model, and whether the daemon lets it be changed.
    property var sup: {
        const j = backend.device_support(pid);
        return j === "" ? ({}) : JSON.parse(j);
    }
    readonly property bool untested: sup.tested === false && sup.supported !== false
    // Writes refused for this device: a read-only daemon, or an untested model not yet allowed.
    readonly property bool writesBlocked: backend.readonly || (sup.allow_writes !== undefined && !sup.allow_writes)
    // Untested and not allowed: the controls are greyed out as well.
    readonly property bool locked: untested && !sup.allow_untested_writes
    property var responded: []
    readonly property bool isMouse: DeviceKinds.family(st.kind || sup.kind) === "mouse"
    signal openStudio()
    signal openLinks()
    signal openMacros()
    // The POWER tab only exists for devices that report power settings.
    readonly property bool hasPower: {
        const s = backend.power(pid);
        return s !== "" && JSON.parse(s).sleep_seconds !== undefined;
    }
    readonly property var tabHelp: ({
        "Info": "What Tsuba knows about this model.",
        "Customize": "Click a key or button to see what it does and change it. Changes are saved in the active profile and written to the device's on-board memory, so they keep working without Tsuba. Remapped keys are shown in the accent colour.",
        "Performance": "Settings that change how the device behaves: sensitivity, polling rate, scrolling, Game Mode and similar. Most are stored on the device's current profile.",
        "Lighting": "Pick a colour and effect for this device, set its brightness, or play a Lighting Studio profile across all your devices.",
        "Power": "Battery level, charging state and power-saving options for this wireless device."
    })
    readonly property var tabs: DeviceKinds.tabsFor(sup, hasPower)
    readonly property var stackOrder: ["Customize", "Performance", "Lighting", "Power", "Info"]
    function tabActive(name) {
        return tabs.indexOf(name) >= 0;
    }

    // "SLOT n" tag of a pinned profile (dimmed while another profile occupies the slot).
    component SlotBadge: Rectangle {
        property int slot: 0
        property bool held: true
        implicitWidth: sbt.implicitWidth + 10
        implicitHeight: 16
        radius: 2
        color: "transparent"
        border.color: held ? Theme.textDim : Theme.borderSoft
        Text {
            id: sbt
            anchors.centerIn: parent
            text: "SLOT " + parent.slot
            color: parent.held ? Theme.textDim : Theme.borderSoft
            font.family: Theme.font
            font.pixelSize: 9
            font.letterSpacing: 1
        }
    }

    // Small padlock (our own drawing): the profile is stored on the device itself, so it works
    // without Tsuba.
    component LockIcon: Item {
        property bool held: true
        implicitWidth: 12
        implicitHeight: 15
        Rectangle {
            x: 2
            y: 0
            width: 8
            height: 9
            radius: 4
            color: "transparent"
            border.width: 1.5
            border.color: parent.held ? Theme.textDim : Theme.borderSoft
        }
        Rectangle {
            x: 0
            y: 6
            width: 12
            height: 9
            radius: 2
            color: parent.held ? Theme.textDim : Theme.borderSoft
        }
    }
    // Names of the games linked to a profile.
    component LinkBadge: Rectangle {
        property var apps: []
        implicitWidth: lbt.implicitWidth + 10
        implicitHeight: 16
        radius: 2
        color: Theme.accentTint
        border.color: Theme.accent
        Text {
            id: lbt
            anchors.centerIn: parent
            width: Math.min(implicitWidth, 130)
            elide: Text.ElideRight
            text: parent.apps.length === 1 ? parent.apps[0].toUpperCase() : i18n("%1 GAMES", parent.apps.length)
            color: Theme.accent
            font.family: Theme.font
            font.pixelSize: 9
            font.letterSpacing: 1
        }
    }

    color: Theme.bg

    function reload() {
        const s = backend.state(pid);
        if (s !== "") {
            st = JSON.parse(s);
        }
        const j = backend.device_support(pid);
        if (j !== "") {
            sup = JSON.parse(j);
        }
    }

    // Which settings groups the device answers (for the "Report this device" text).
    function probe() {
        const out = [];
        [["profiles", "profiles"], ["performance", "performance"], ["power", "power"], ["game_mode", "game mode"], ["lighting_power", "lighting power"]].forEach(p => {
            const r = p[0] === "performance" ? backend.performance(pid, 1) : backend[p[0]](pid);
            if (r !== "" && r !== undefined) {
                out.push(p[1]);
            }
        });
        if (st.lighting && st.lighting.length) {
            out.push("lighting");
        }
        responded = out;
    }

    Component.onCompleted: {
        reload();
        loadProfiles();
        if (untested) {
            probe();
        }
        if (backend.env_var("TSUBA_PROFILE_MENU") === "1") {   // headless UI check
            profileMenu.open();
        }
        if (backend.env_var("TSUBA_MORE_MENU") === "1") {
            moreMenu.open();
        }
        if (backend.env_var("TSUBA_SYNAPSE") !== "") {   // headless UI check
            synapseDlg.open("device:" + pid);
        }
    }

    // The refresh button: re-read the device (the daemon drops its cached reads first).
    Connections {
        target: Theme
        function onDataRevChanged() {
            page.reload();
            page.loadProfiles();
        }
    }

    Connections {
        target: page.backend
        ignoreUnknownSignals: true
        // The profile button under the mouse (or a profile key) switches on-board slots; the
        // library profile pinned to (or held by) the new slot becomes the active one.
        function onHardware_event(epid, event) {
            if (epid !== page.pid) {
                return;
            }
            // The Basilisk doesn't send a "profile switch" notification for its profile button;
            // it re-sends settings (scroll mode, battery) instead, so re-check after any burst.
            if (JSON.parse(event).kind === "notification") {
                profileCheck.restart();
            }
        }
        function onLibrary_changed(epid) {
            if (epid === page.pid && page.switching === "") {
                profileCheck.restart();
            }
        }
        function onLibrary_activated(epid, ok, result) {
            if (epid !== page.pid) {
                return;
            }
            const p = page.profileById(page.switching);
            const name = p ? p.name : "The profile";
            page.switching = "";
            if (ok) {
                const r = JSON.parse(result);
                const n = r.writes || 0;
                page.profileStatus = name + " is active" + (n > 0 ? " (" + n + (n === 1 ? " change" : " changes") + " written)" : "")
                    + (r.warnings && r.warnings.length ? "; " + r.warnings.length + " setting(s) couldn't be written: " + r.warnings[0] : "");
            } else {
                page.profileStatus = "Failed: " + result;
            }
            page.loadProfiles();
            page.libRev++;
            Theme.profilesRev++;
            const del = page.pendingDelete;
            page.pendingDelete = "";
            if (del !== "" && ok) {
                page.deleteProfile(del);
            }
        }
    }

    Timer {
        id: profileCheck
        interval: 300
        onTriggered: {
            if (page.loadProfiles()) {
                page.profileStatus = page.currentName() + " is active";
                page.libRev++;
            }
        }
    }

    // Inline name entry / confirmation for Add / Duplicate / Rename / Delete in the "•••" menu.
    property string nameMode: ""

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // Row 3: profile picker + menu | page tabs | help
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 60
            color: Theme.bg
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 2
                color: Theme.line
            }

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 20
                anchors.rightMargin: 20
                spacing: 10

                // Profile stack icon
                Item {
                    visible: page.hasProfiles
                    Layout.preferredWidth: 26
                    Layout.preferredHeight: 26
                    Repeater {
                        model: 3
                        delegate: Rectangle {
                            required property int index
                            x: 3
                            y: 3 + index * 6
                            width: 20
                            height: 6
                            radius: 1
                            color: "transparent"
                            border.color: Theme.text
                            border.width: 1
                        }
                    }
                }

                Rectangle {
                    id: profileButton
                    objectName: "profileButton"
                    visible: page.hasProfiles
                    Layout.preferredWidth: 300
                    Layout.preferredHeight: 34
                    color: Theme.card
                    border.color: profileMenu.opened || pbh.hovered ? Theme.accent : Theme.border
                    radius: 2
                    Text {
                        anchors.left: parent.left
                        anchors.leftMargin: 12
                        anchors.right: badges.left
                        anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        text: page.switching !== "" ? "Switching to " + (page.profileById(page.switching) || { name: "…" }).name + "…" : page.currentName()
                        color: page.switching !== "" ? Theme.textDim : Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.fontSize
                        font.italic: page.switching !== ""
                        elide: Text.ElideRight
                    }
                    Row {
                        id: badges
                        anchors.right: arrow.left
                        anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 6
                        SlotBadge {
                            visible: page.switching === "" && !!page.current && page.current.pinned_slot !== null && page.current.pinned_slot !== undefined
                            slot: page.current && page.current.pinned_slot ? page.current.pinned_slot : 0
                        }
                        Text {
                            visible: page.switching === "" && !page.current && page.lib.active_slot > 0
                            text: "ON-BOARD"
                            color: Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: 10
                            font.letterSpacing: 1
                        }
                    }
                    Text {
                        id: arrow
                        anchors.right: parent.right
                        anchors.rightMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        text: profileMenu.opened ? "▴" : "▾"
                        color: Theme.text
                        font.pixelSize: 12
                    }
                    HoverHandler {
                        id: pbh
                        cursorShape: Qt.PointingHandCursor
                    }
                    TapHandler {
                        onTapped: {
                            page.loadProfiles();
                            page.profileStatus = "";
                            profileMenu.open();
                        }
                    }

                    QQC2.Popup {
                        id: profileMenu
                        y: parent.height - 1
                        width: parent.width
                        padding: 1
                        background: Rectangle {
                            color: Theme.card
                            border.color: Theme.accent
                        }
                        contentItem: ColumnLayout {
                            spacing: 0
                            ListView {
                                id: profileList
                                objectName: "profileList"
                                Layout.fillWidth: true
                                Layout.preferredHeight: Math.min(count, 12) * 36
                                clip: true
                                boundsBehavior: Flickable.StopAtBounds
                                model: page.lib.profiles
                                QQC2.ScrollBar.vertical: QQC2.ScrollBar {
                                    policy: profileList.count > 12 ? QQC2.ScrollBar.AlwaysOn : QQC2.ScrollBar.AsNeeded
                                }
                                delegate: Rectangle {
                                    required property var modelData
                                    readonly property bool isActive: modelData.id === page.lib.active
                                    width: ListView.view.width
                                    height: 36
                                    color: rh.hovered ? "#2d2d2d" : "transparent"
                                    RowLayout {
                                        anchors.fill: parent
                                        anchors.leftMargin: 12
                                        anchors.rightMargin: profileList.count > 12 ? 22 : 12
                                        spacing: 8
                                        Text {
                                            Layout.fillWidth: true
                                            text: modelData.name
                                            color: isActive ? Theme.accent : Theme.text
                                            font.family: Theme.font
                                            font.pixelSize: Theme.fontSize
                                            elide: Text.ElideRight
                                        }
                                        LinkBadge {
                                            objectName: "linkBadge"
                                            visible: !!page.linkedApps[modelData.id]
                                            apps: page.linkedApps[modelData.id] || []
                                        }
                                        LockIcon {
                                            objectName: "lockIcon"
                                            visible: modelData.pinned_slot !== null && modelData.pinned_slot !== undefined
                                            held: modelData.in_slot
                                        }
                                        SlotBadge {
                                            visible: modelData.pinned_slot !== null && modelData.pinned_slot !== undefined
                                            slot: modelData.pinned_slot || 0
                                            held: modelData.in_slot
                                        }
                                        Text {
                                            visible: isActive
                                            text: "ACTIVE"
                                            color: Theme.accent
                                            font.family: Theme.font
                                            font.pixelSize: 10
                                            font.letterSpacing: 1
                                        }
                                    }
                                    HoverHandler {
                                        id: rh
                                        cursorShape: page.writesBlocked ? Qt.ArrowCursor : Qt.PointingHandCursor
                                    }
                                    // Like Synapse: picking a profile makes it the active one.
                                    TapHandler {
                                        onTapped: {
                                            profileMenu.close();
                                            page.activate(modelData.id);
                                        }
                                    }
                                }
                            }
                            Text {
                                visible: !page.current && page.lib.active_slot > 0
                                Layout.fillWidth: true
                                Layout.margins: 10
                                text: "The device runs on-board profile " + page.lib.active_slot + ", which isn't in the library. Pick a profile to switch."
                                color: Theme.textDim
                                font.family: Theme.font
                                font.pixelSize: Theme.small
                                wrapMode: Text.Wrap
                            }
                            Text {
                                visible: page.writesBlocked
                                Layout.fillWidth: true
                                Layout.margins: 10
                                text: "Tsuba may not change this device, so profiles can't be switched here."
                                color: Theme.textDim
                                font.family: Theme.font
                                font.pixelSize: Theme.small
                                wrapMode: Text.Wrap
                            }
                        }
                    }
                }

                // "•••" menu: Add / Rename / Duplicate / Delete / Pin
                Rectangle {
                    visible: page.hasProfiles
                    Layout.preferredWidth: 34
                    Layout.preferredHeight: 34
                    radius: 2
                    color: mh.hovered || moreMenu.opened ? "#2d2d2d" : "transparent"
                    border.color: moreMenu.opened ? Theme.accent : "transparent"
                    Text {
                        anchors.centerIn: parent
                        text: "•••"
                        color: Theme.text
                        font.pixelSize: 12
                    }
                    HoverHandler {
                        id: mh
                        cursorShape: Qt.PointingHandCursor
                    }
                    TapHandler {
                        onTapped: {
                            page.loadProfiles();
                            page.profileStatus = "";
                            page.nameMode = "";
                            moreMenu.open();
                        }
                    }

                    QQC2.Popup {
                        id: moreMenu
                        objectName: "moreMenu"
                        y: parent.height + 2
                        width: 280
                        padding: 1
                        background: Rectangle {
                            color: Theme.card
                            border.color: Theme.border
                        }
                        contentItem: ColumnLayout {
                            spacing: 0
                            component MenuRow: Rectangle {
                                id: mrow
                                property string caption: ""
                                signal picked()
                                Layout.fillWidth: true
                                Layout.preferredHeight: 36
                                visible: page.nameMode === ""
                                color: mrh.hovered && enabled ? "#2d2d2d" : "transparent"
                                opacity: enabled ? 1 : 0.4
                                Text {
                                    anchors.left: parent.left
                                    anchors.leftMargin: 12
                                    anchors.right: parent.right
                                    anchors.rightMargin: 12
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: mrow.caption
                                    color: Theme.text
                                    font.family: Theme.font
                                    font.pixelSize: Theme.fontSize
                                    elide: Text.ElideRight
                                }
                                HoverHandler {
                                    id: mrh
                                    cursorShape: Qt.PointingHandCursor
                                }
                                TapHandler {
                                    enabled: mrow.enabled
                                    onTapped: mrow.picked()
                                }
                            }
                            MenuRow {
                                objectName: "menuAdd"
                                caption: "Add"
                                enabled: !page.writesBlocked && page.switching === ""
                                onPicked: {
                                    nameField.text = "Profile " + ((page.lib.profiles || []).length + 1);
                                    page.nameMode = "new";
                                    nameField.forceActiveFocus();
                                }
                            }
                            MenuRow {
                                objectName: "menuFactory"
                                caption: i18n("New from factory defaults")
                                enabled: !page.writesBlocked && page.switching === ""
                                onPicked: {
                                    nameField.text = "Factory defaults";
                                    page.nameMode = "factory";
                                    nameField.forceActiveFocus();
                                }
                            }
                            MenuRow {
                                caption: "Rename"
                                enabled: !!page.current
                                onPicked: {
                                    nameField.text = page.current.name;
                                    page.nameMode = "rename";
                                    nameField.forceActiveFocus();
                                }
                            }
                            MenuRow {
                                caption: "Duplicate"
                                enabled: !!page.current && !page.writesBlocked && page.switching === ""
                                onPicked: {
                                    nameField.text = page.current.name + " copy";
                                    page.nameMode = "duplicate";
                                    nameField.forceActiveFocus();
                                }
                            }
                            MenuRow {
                                objectName: "menuDelete"
                                caption: "Delete"
                                enabled: !!page.current && !page.writesBlocked && page.switching === "" && !!page.fallbackFor(page.current.id)
                                onPicked: page.nameMode = "delete"
                            }
                            MenuRow {
                                objectName: "menuPin"
                                readonly property bool pinned: !!page.current && page.current.pinned_slot !== null && page.current.pinned_slot !== undefined
                                readonly property var free: page.lib.free_slots || []
                                caption: pinned ? "Unpin from slot " + page.current.pinned_slot
                                    : free.length > 0 ? "Pin to device (slot " + free[0] + ")" : "Pin to device (no free slot)"
                                enabled: !!page.current && !page.writesBlocked && page.switching === "" && (pinned || free.length > 0)
                                onPicked: {
                                    const p = page.current;
                                    if (pinned) {
                                        page.profileAction(page.backend.unpin_library_profile(page.pid, p.id), "“" + p.name + "” is no longer pinned");
                                    } else {
                                        page.profileStatus = "Writing “" + p.name + "” to the device…";
                                        const s = page.backend.pin_library_profile(page.pid, p.id, 0);
                                        page.profileAction(s > 0, "“" + p.name + "” pinned to on-board slot " + s + ": the profile button reaches it without Tsuba");
                                        page.libRev++;
                                    }
                                }
                            }
                            MenuRow {
                                objectName: "menuLighting"
                                readonly property bool has: !!page.current && !!page.current.lighting
                                caption: has ? i18n("✓ Lighting follows this profile (%1)", page.current.lighting.effect) : i18n("Lighting follows this profile")
                                enabled: !!page.current && !page.writesBlocked && page.switching === ""
                                onPicked: page.setProfileLighting(!has)
                            }
                            Rectangle {
                                visible: page.nameMode === ""
                                Layout.fillWidth: true
                                Layout.preferredHeight: 1
                                color: Theme.borderSoft
                            }
                            MenuRow {
                                objectName: "menuImport"
                                caption: i18n("Import…")
                                enabled: !page.writesBlocked
                                onPicked: {
                                    moreMenu.close();
                                    importDlg.currentFolder = page.lp.homeUrl();
                                    importDlg.open();
                                }
                            }
                            MenuRow {
                                objectName: "menuExport"
                                caption: i18n("Export…")
                                enabled: !!page.current
                                onPicked: {
                                    moreMenu.close();
                                    exportDlg.currentFolder = page.lp.homeUrl();
                                    exportDlg.currentFile = page.lp.homeUrl() + "/" + page.fileNameFor(page.current);
                                    exportDlg.open();
                                }
                            }
                            MenuRow {
                                objectName: "menuExportMany"
                                caption: i18n("Export several…")
                                enabled: (page.lib.profiles || []).length > 0
                                onPicked: {
                                    page.exportSel = ({});
                                    page.nameMode = "exportmany";
                                }
                            }
                            Rectangle {
                                visible: page.nameMode === ""
                                Layout.fillWidth: true
                                Layout.preferredHeight: 1
                                color: Theme.borderSoft
                            }
                            MenuRow {
                                objectName: "menuLinkedGames"
                                caption: "Linked games"
                                onPicked: {
                                    moreMenu.close();
                                    page.openLinks();
                                }
                            }
                            MenuRow {
                                objectName: "menuSynapseImport"
                                caption: "Import from Synapse…"
                                onPicked: {
                                    moreMenu.close();
                                    synapseDlg.open("device:" + page.pid);
                                }
                            }
                            // Export several profiles: tick them, then choose a folder.
                            ColumnLayout {
                                objectName: "exportManyPanel"
                                visible: page.nameMode === "exportmany"
                                Layout.fillWidth: true
                                Layout.margins: 12
                                spacing: 6
                                Text {
                                    text: i18n("Export profiles")
                                    color: Theme.textDim
                                    font.family: Theme.font
                                    font.pixelSize: Theme.small
                                }
                                SynCheckBox {
                                    objectName: "exportAll"
                                    text: i18n("Select all")
                                    checked: page.exportCount === (page.lib.profiles || []).length && page.exportCount > 0
                                    onToggled: page.selectAllForExport(checked)
                                }
                                Repeater {
                                    model: page.lib.profiles || []
                                    delegate: SynCheckBox {
                                        required property var modelData
                                        objectName: "export_" + modelData.id
                                        text: modelData.name
                                        checked: !!page.exportSel[modelData.id]
                                        onToggled: {
                                            const m = Object.assign({}, page.exportSel);
                                            m[modelData.id] = checked;
                                            page.exportSel = m;
                                        }
                                    }
                                }
                                RowLayout {
                                    spacing: 8
                                    SynButton {
                                        text: "Cancel"
                                        implicitWidth: 80
                                        onClicked: page.nameMode = ""
                                    }
                                    SynButton {
                                        objectName: "exportChosen"
                                        text: "Export…"
                                        primary: true
                                        implicitWidth: 90
                                        enabled: page.exportCount > 0
                                        onClicked: {
                                            page.nameMode = "";
                                            moreMenu.close();
                                            exportDirDlg.currentFolder = page.lp.homeUrl();
                                            exportDirDlg.open();
                                        }
                                    }
                                }
                            }
                            // Delete confirmation.
                            ColumnLayout {
                                visible: page.nameMode === "delete"
                                Layout.fillWidth: true
                                Layout.margins: 12
                                spacing: 8
                                Text {
                                    Layout.fillWidth: true
                                    text: page.current ? "Delete “" + page.current.name + "”?" + (page.fallbackFor(page.current.id) ? " Tsuba switches to “" + page.fallbackFor(page.current.id).name + "” first." : "")
                                        + (page.current.pinned_slot ? " Its on-board slot is freed." : "") : ""
                                    color: Theme.text
                                    font.family: Theme.font
                                    font.pixelSize: Theme.small
                                    wrapMode: Text.Wrap
                                }
                                RowLayout {
                                    spacing: 8
                                    SynButton {
                                        text: "Cancel"
                                        implicitWidth: 80
                                        onClicked: page.nameMode = ""
                                    }
                                    SynButton {
                                        objectName: "confirmDelete"
                                        text: "Delete"
                                        primary: true
                                        implicitWidth: 80
                                        onClicked: {
                                            page.nameMode = "";
                                            page.deleteProfile(page.current.id);
                                        }
                                    }
                                }
                            }
                            // Name entry for new / duplicate / rename.
                            ColumnLayout {
                                visible: ["new", "rename", "duplicate", "factory"].indexOf(page.nameMode) >= 0
                                Layout.fillWidth: true
                                Layout.margins: 12
                                spacing: 8
                                Text {
                                    text: page.nameMode === "rename" ? "Rename profile" : (page.nameMode === "duplicate" ? "Duplicate as" : (page.nameMode === "factory" ? i18n("New profile with factory settings") : "New profile"))
                                    color: Theme.textDim
                                    font.family: Theme.font
                                    font.pixelSize: Theme.small
                                }
                                QQC2.TextField {
                                    id: nameField
                                    objectName: "profileNameField"
                                    Layout.fillWidth: true
                                    onAccepted: okButton.clicked()
                                }
                                RowLayout {
                                    spacing: 8
                                    SynButton {
                                        text: "Cancel"
                                        implicitWidth: 80
                                        onClicked: page.nameMode = ""
                                    }
                                    SynButton {
                                        id: okButton
                                        objectName: "profileNameSave"
                                        text: "Save"
                                        primary: true
                                        implicitWidth: 72
                                        enabled: nameField.text.trim() !== ""
                                        onClicked: {
                                            const n = nameField.text.trim();
                                            if (page.nameMode === "rename") {
                                                page.profileAction(page.backend.rename_library_profile(page.pid, page.current.id, n), "Renamed");
                                            } else if (page.nameMode === "factory") {
                                                const id = page.lp.call("CreateFactoryProfile", [page.pid, n]);
                                                page.profileAction(id !== null, i18n("Created “%1” with factory settings", n));
                                                if (id !== null) {
                                                    page.activate(id);
                                                } else {
                                                    page.profileStatus = "Failed: " + page.lp.error;
                                                }
                                            } else {
                                                const id = page.nameMode === "duplicate" ? page.backend.duplicate_library_profile(page.pid, page.current.id, n) : page.backend.create_library_profile(page.pid, n, "");
                                                page.profileAction(id !== "", "Created “" + n + "”");
                                                if (id !== "") {
                                                    // Like Synapse: the new profile becomes the active one.
                                                    page.activate(id);
                                                }
                                            }
                                            page.nameMode = "";
                                        }
                                    }
                                }
                            }
                            Text {
                                visible: page.profileStatus !== ""
                                Layout.fillWidth: true
                                Layout.margins: 10
                                Layout.topMargin: 4
                                text: page.profileStatus
                                color: page.profileStatus.startsWith("Failed") ? Theme.warn : Theme.accent
                                font.family: Theme.font
                                font.pixelSize: Theme.small
                                wrapMode: Text.Wrap
                            }
                        }
                    }
                }

                // Result of the last profile switch / action (the menus close on a pick).
                Text {
                    objectName: "profileStatus"
                    visible: page.profileStatus !== "" && !moreMenu.opened
                    Layout.maximumWidth: 360
                    text: page.profileStatus
                    color: page.profileStatus.startsWith("Failed") ? Theme.warn : Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    elide: Text.ElideRight
                }

                Item {
                    Layout.fillWidth: true
                }
                HelpButton {
                    size: 22
                    text: page.tabHelp[page.tabs[page.tabIndex]] || ""
                }
            }

            // Page tabs, centred over the window
            SynPillTabs {
                id: pageTabs
                anchors.centerIn: parent
                outlined: false
                upper: true
                fontSize: 14
                model: page.tabs
                currentIndex: page.tabIndex
                onActivated: i => page.tabIndex = i
            }
        }

        UntestedBanner {
            objectName: "untestedBanner"
            Layout.fillWidth: true
            backend: page.backend
            pid: page.pid
            support: page.sup
            st: page.st
            responded: page.responded
            onUpdated: s => {
                page.sup = s;
                page.reload();
                page.loadProfiles();
                Theme.dataRev++;
            }
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: Math.max(0, page.stackOrder.indexOf(page.tabs[page.tabIndex]))

            Loader {
                active: page.tabActive("Customize")
                sourceComponent: CustomizeTab {
                    backend: page.backend
                    pid: page.pid
                    profile: page.profile
                    profileId: page.lib.active || ""
                    profileCount: (page.lib.profiles || []).length
                    rev: page.libRev
                    isMouse: page.isMouse
                    readOnly: page.writesBlocked
                    onOpenMacros: page.openMacros()
                }
            }
            Loader {
                active: page.tabActive("Performance")
                sourceComponent: PerformanceTab {
                    backend: page.backend
                    pid: page.pid
                    profile: page.profile
                    rev: page.libRev
                    st: page.st
                    readOnly: page.writesBlocked
                    locked: page.locked
                    onChanged: page.reload()
                }
            }
            Loader {
                active: page.tabActive("Lighting")
                sourceComponent: LightingPage {
                    backend: page.backend
                    pid: page.pid
                    st: page.st
                    support: page.sup
                    readOnly: page.writesBlocked
                    locked: page.locked
                    onChanged: page.reload()
                    onLaunchStudio: page.openStudio()
                }
            }
            Loader {
                active: page.tabActive("Power")
                sourceComponent: PerformanceTab {
                    mode: "power"
                    backend: page.backend
                    pid: page.pid
                    profile: page.profile
                    st: page.st
                    readOnly: page.writesBlocked
                    locked: page.locked
                    onChanged: page.reload()
                }
            }
            Loader {
                active: page.tabActive("Info")
                sourceComponent: DeviceInfoPage {
                    support: page.sup
                }
            }
        }
    }

    SynapseImportDialog {
        id: synapseDlg
        objectName: "synapseImportDialog"
        anchors.fill: parent
        z: 50
        backend: page.backend
        onImported: {
            page.loadProfiles();
            Theme.profilesRev++;
        }
    }
}
