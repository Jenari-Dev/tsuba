import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// Linked games (Synapse's game/app linking): while a linked program runs, devices switch to
// the chosen on-board profiles and Chroma profile; everything returns when it closes.
Rectangle {
    id: page

    required property var backend

    property var links: []
    property var active: null
    property int sel: -1
    property var programs: []
    property var chromaNames: []
    readonly property var devs: JSON.parse(backend.devices || "[]")
    property var deviceProfiles: ({})   // pid -> [{id, name}]

    color: Theme.bg

    function load() {
        const j = JSON.parse(backend.app_links() || "{\"links\": []}");
        links = j.links;
        active = j.active;
        if (sel >= links.length) {
            sel = links.length - 1;
        }
    }
    function loadChoices() {
        programs = JSON.parse(backend.running_programs() || "[]");
        const c = JSON.parse(backend.chroma_profiles() || "{\"profiles\": []}");
        chromaNames = c.profiles.map(p => p.name);
        const dp = {};
        devs.forEach(d => {
            // Library profiles (ids); on-board slot numbers in old links show as "Slot n".
            const p = JSON.parse(backend.library_profiles(d.pid) || "{\"profiles\": []}");
            dp[d.pid] = p.profiles.map(x => ({ id: x.id, name: x.name + (x.pinned_slot ? "  (slot " + x.pinned_slot + ")" : "") }));
        });
        deviceProfiles = dp;
    }
    function store(list) {
        links = list;
        backend.set_app_links(JSON.stringify({ links: list }));
    }
    function edit(fn) {
        const l = JSON.parse(JSON.stringify(links));
        fn(l[sel]);
        store(l);
    }
    function scanGames() {
        scanDialog.open(JSON.parse(backend.installed_games() || "[]"));
        scanDialog.forceActiveFocus();
    }
    // Create links from scanned games exactly like manual entries; `matches` finds them by folder.
    function addGames(chosen) {
        const first = links.length;
        const add = chosen.map(g => ({
            name: g.name,
            matches: scanDialog.matchFor(g),
            enabled: true,
            device_profiles: {},
            chroma: null,
            art: g.art || ""
        })).filter(l => l.matches !== "");
        if (add.length) {
            store(links.concat(add));
            sel = first;
            loadChoices();
        }
        scanDialog.visible = false;
    }
    function label(l) {
        return l.name !== "" ? l.name : (l.matches !== "" ? l.matches : "New link");
    }

    Component.onCompleted: {
        load();
        loadChoices();
        if (backend.env_var && backend.env_var("TSUBA_GAMESCAN") === "1") {   // headless UI check
            scanGames();
        }
    }
    // Keep the profile lists fresh: when the page is shown, when the device list changes and
    // after profiles are created / renamed / deleted elsewhere in the app.
    function reloadAll() {
        load();
        loadChoices();
    }
    onVisibleChanged: if (visible) reloadAll()
    onDevsChanged: if (visible) loadChoices()
    Connections {
        target: Theme
        function onProfilesRevChanged() {
            page.loadChoices();
        }
    }
    Connections {
        target: page.backend
        function onDevices_changed_on_bus() {
            page.loadChoices();
        }
    }
    Timer {
        interval: 3000
        repeat: true
        running: page.visible
        onTriggered: page.active = JSON.parse(page.backend.app_links() || "{}").active
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // ---- Links
        Rectangle {
            Layout.preferredWidth: 320
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
                        caption: "Linked games"
                    }
                    HelpButton {
                        Layout.leftMargin: 6
                        size: 16
                        text: "Link a game or app to device profiles and a Lighting Studio profile. While it runs, your devices switch to those; when it closes they switch back. Games are found by matching their process name, command line or install folder (Steam and Proton games work too)."
                    }
                    Item {
                        Layout.fillWidth: true
                    }
                    SynButton {
                        objectName: "scanGamesButton"
                        text: "Scan for games"
                        implicitWidth: 130
                        onClicked: page.scanGames()
                    }
                    SynButton {
                        text: "+ Add"
                        implicitWidth: 72
                        onClicked: {
                            page.loadChoices();
                            page.store(page.links.concat([{ name: "", matches: "", enabled: true, device_profiles: {}, chroma: null }]));
                            page.sel = page.links.length - 1;
                        }
                    }
                }
                Text {
                    Layout.fillWidth: true
                    text: page.active ? "Running now: " + page.active : "No linked game running"
                    color: page.active ? Theme.accent : Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    wrapMode: Text.Wrap
                }
                Repeater {
                    model: page.links
                    delegate: Rectangle {
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        Layout.preferredHeight: 54
                        color: page.sel === index ? Theme.accentTint : (lh.hovered ? "#14ffffff" : Theme.bg)
                        border.color: page.sel === index ? Theme.accent : Theme.borderSoft
                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 12
                            anchors.rightMargin: 10
                            spacing: 10
                            Image {
                                visible: !!modelData.art
                                Layout.preferredWidth: 30
                                Layout.preferredHeight: 42
                                source: modelData.art || ""
                                fillMode: Image.PreserveAspectCrop
                                sourceSize.width: 60
                                sourceSize.height: 84
                                asynchronous: true
                            }
                            Column {
                                Layout.fillWidth: true
                                Text {
                                    width: parent.width
                                    textFormat: Text.PlainText
                                    text: page.label(modelData)
                                    color: modelData.enabled ? Theme.textBright : Theme.textMuted
                                    font.family: Theme.font
                                    font.pixelSize: Theme.fontSize
                                    elide: Text.ElideRight
                                }
                                Text {
                                    width: parent.width
                                    textFormat: Text.PlainText
                                    text: modelData.matches.indexOf("dir:") === 0 ? "folder " + modelData.matches.slice(4) : (modelData.matches !== "" ? "matches \"" + modelData.matches + "\"" : "pick a program")
                                    color: Theme.textDim
                                    font.family: Theme.font
                                    font.pixelSize: 11
                                    elide: Text.ElideRight
                                }
                            }
                            Text {
                                visible: page.active !== null && page.active === page.label(modelData)
                                text: "RUNNING"
                                color: Theme.accent
                                font.family: Theme.font
                                font.pixelSize: 10
                                font.letterSpacing: 1
                            }
                        }
                        HoverHandler {
                            id: lh
                            cursorShape: Qt.PointingHandCursor
                        }
                        TapHandler {
                            onTapped: page.sel = index
                        }
                    }
                }
                Item {
                    Layout.fillHeight: true
                }
                Text {
                    Layout.fillWidth: true
                    text: "When several linked games run, the one higher in this list wins."
                    color: Theme.textMuted
                    font.family: Theme.font
                    font.pixelSize: 11
                    wrapMode: Text.Wrap
                }
            }
        }

        // ---- Editor
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            Column {
                visible: page.sel < 0
                anchors.centerIn: parent
                spacing: 10
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: "LINKED GAMES"
                    color: Theme.text
                    font.family: Theme.font
                    font.pixelSize: Theme.heading
                    font.letterSpacing: 2
                }
                Text {
                    width: 480
                    horizontalAlignment: Text.AlignHCenter
                    text: "Link a game or app to device profiles and a Lighting Studio profile. They switch on when it starts and switch back when it closes. Steam/Proton games work too."
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.Wrap
                }
            }

            ColumnLayout {
                visible: page.sel >= 0 && page.sel < page.links.length
                anchors.fill: parent
                anchors.margins: 28
                spacing: 16
                readonly property var cur: page.sel >= 0 ? page.links[page.sel] : null

                RowLayout {
                    spacing: 12
                    QQC2.TextField {
                        Layout.preferredWidth: 320
                        placeholderText: "Name (optional)"
                        text: parent.parent.cur ? parent.parent.cur.name : ""
                        onEditingFinished: page.edit(l => l.name = text)
                    }
                    SynCheckBox {
                        text: "Enabled"
                        checked: !!parent.parent.cur && parent.parent.cur.enabled
                        onToggled: page.edit(l => l.enabled = checked)
                    }
                    Item {
                        Layout.fillWidth: true
                    }
                    SynButton {
                        text: "Delete"
                        onClicked: {
                            const l = page.links.slice();
                            l.splice(page.sel, 1);
                            page.store(l);
                            page.sel = Math.min(page.sel, l.length - 1);
                        }
                    }
                }

                SectionLabel {
                    caption: "Program"
                }
                RowLayout {
                    spacing: 10
                    QQC2.ComboBox {
                        Layout.preferredWidth: 320
                        model: ["Pick a running program…"].concat(page.programs)
                        onActivated: i => {
                            if (i > 0) {
                                const p = page.programs[i - 1];
                                page.edit(l => {
                                    l.matches = p;
                                    if (l.name === "") l.name = p.replace(/\.exe$/i, "");
                                });
                            }
                        }
                    }
                    SynButton {
                        text: "Refresh"
                        implicitWidth: 84
                        onClicked: page.loadChoices()
                    }
                }
                RowLayout {
                    spacing: 10
                    Text {
                        text: "or match text"
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                    }
                    QQC2.TextField {
                        Layout.preferredWidth: 320
                        placeholderText: "e.g. cs2  or  eldenring.exe"
                        text: parent.parent.cur ? parent.parent.cur.matches : ""
                        onEditingFinished: page.edit(l => l.matches = text.trim())
                    }
                }
                Text {
                    Layout.fillWidth: true
                    text: "Matched against running process names and command lines (case doesn't matter). Start the game first to pick it from the list. Text starting with dir: followed by a folder matches any program running from inside that folder."
                    color: Theme.textMuted
                    font.family: Theme.font
                    font.pixelSize: 11
                    wrapMode: Text.Wrap
                }

                SectionLabel {
                    caption: "While it runs"
                }
                Repeater {
                    model: page.devs
                    delegate: RowLayout {
                        required property var modelData
                        spacing: 12
                        readonly property var opts: page.deviceProfiles[modelData.pid] || []
                        readonly property var cur: page.sel >= 0 && page.links[page.sel] ? page.links[page.sel] : null
                        Text {
                            Layout.preferredWidth: 200
                            text: modelData.name.replace(" (wired)", "")
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: Theme.fontSize
                        }
                        readonly property var want: cur ? cur.device_profiles[modelData.pid] : undefined
                        // A slot number saved before the profile library (tsubad migrates it).
                        readonly property bool legacy: typeof want === "number"
                        QQC2.ComboBox {
                            Layout.preferredWidth: 260
                            model: ["Don't change profile"].concat(opts.map(o => o.name)).concat(parent.legacy ? ["On-board slot " + parent.want] : [])
                            currentIndex: parent.want === undefined ? 0 : (parent.legacy ? opts.length + 1 : 1 + opts.findIndex(o => o.id === parent.want))
                            onActivated: i => page.edit(l => {
                                if (i === 0) {
                                    delete l.device_profiles[modelData.pid];
                                } else if (i - 1 < opts.length) {
                                    l.device_profiles[modelData.pid] = opts[i - 1].id;
                                }
                            })
                        }
                    }
                }
                RowLayout {
                    spacing: 12
                    Text {
                        Layout.preferredWidth: 200
                        text: "Studio lighting"
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.fontSize
                    }
                    QQC2.ComboBox {
                        Layout.preferredWidth: 260
                        readonly property var cur: page.sel >= 0 && page.links[page.sel] ? page.links[page.sel] : null
                        model: ["Don't change lighting"].concat(page.chromaNames)
                        currentIndex: cur && cur.chroma ? 1 + page.chromaNames.indexOf(cur.chroma) : 0
                        onActivated: i => page.edit(l => l.chroma = i === 0 ? null : page.chromaNames[i - 1])
                    }
                }
                Item {
                    Layout.fillHeight: true
                }
            }
        }
    }

    GameScanDialog {
        id: scanDialog
        objectName: "gameScanDialog"
        anchors.fill: parent
        z: 50
        links: page.links
        onAddSelected: chosen => page.addGames(chosen)
        onClosed: visible = false
    }
}
