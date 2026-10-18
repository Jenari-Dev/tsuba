import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Dialogs

// Game lighting: games that support Razer Chroma light up your devices. The daemon runs the
// local server those games talk to; Windows games in Wine / Proton also need Tsuba's bridge
// library in their prefix, which this page installs and removes.
Rectangle {
    id: page

    required property var backend

    property var gl: ({ available: false, running: false, enabled: false, web: false, status: {} })
    property var scan: ({ dll_dir: "", auto: false, prefixes: [], auto_installed: [] })
    property string message: ""
    property bool messageOk: true
    property bool testing: false
    property string testResult: ""
    property bool testOk: true
    // Per-application rules: {only_allowed, apps: [{key, name, exe, rule, allowed, last_seen, connected}]}.
    property var rules: ({ only_allowed: false, apps: [] })

    color: Theme.bg

    readonly property var sessions: (gl.status && gl.status.sessions) || []
    // The session that is lighting devices right now (the newest wins in the daemon).
    readonly property var controlling: {
        const s = sessions.filter(x => (x.devices_active || []).length > 0);
        return s.length ? s[s.length - 1] : null;
    }
    readonly property string headline: {
        if (!gl.available) {
            return gl.running ? "Your tsubad doesn't have game lighting yet" : "tsubad isn't running";
        }
        if (!gl.enabled) {
            return "Game lighting is off";
        }
        if (controlling) {
            return (controlling.title || "A game") + " is controlling your lighting";
        }
        if (sessions.length) {
            return (sessions[sessions.length - 1].title || "A game") + " is connected";
        }
        return "Waiting for a game";
    }
    readonly property string subline: {
        if (!gl.available) {
            return gl.running ? "Update Tsuba and restart tsubad (systemctl --user restart tsubad) to use game lighting." : "Start it with: systemctl --user start tsubad";
        }
        if (!gl.enabled) {
            return "Games can't change your lighting. Turn it on to let them.";
        }
        if (controlling) {
            return "Devices: " + controlling.devices_active.map(page.deviceName).join(", ") + ". Your own lighting comes back when the game closes.";
        }
        if (sessions.length) {
            return "None of your devices is showing its lighting right now.";
        }
        return "Start a game that supports Razer Chroma. It takes over your lighting while it runs.";
    }

    function deviceName(pid) {
        const d = JSON.parse(backend.devices || "[]").find(x => x.pid === pid);
        return d ? d.name.replace(" (wired)", "") : pid;
    }
    function reloadStatus() {
        gl = JSON.parse(backend.game_lighting() || "{}");
        reloadRules();
    }
    property string rulesJson: ""
    function reloadRules() {
        if (typeof backend.studio_call === "function") {
            // Only rebuild the list when something changed (the safety-net timer calls this).
            const j = backend.studio_call("apps", "") || "{}";
            if (j !== rulesJson) {
                rulesJson = j;
                rules = JSON.parse(j);
            }
        }
    }
    // rule: "allow" | "block" | "default"
    function setRule(key, rule) {
        const r = JSON.parse(backend.studio_call("set_app", JSON.stringify({ key: key, rule: rule })) || "{}");
        if (r.ok === false) {
            say(false, r.error || i18n("Couldn't change the rule."));
        }
        // After the segmented control has settled (it is rebuilt with the list).
        Qt.callLater(() => Qt.callLater(reloadStatus));
    }
    function setOnlyAllowed(on) {
        const r = JSON.parse(backend.studio_call("only_allowed", on ? "true" : "false") || "{}");
        if (r.ok === false) {
            say(false, r.error || i18n("Couldn't change the setting."));
        }
        reloadStatus();
    }
    function reloadPrefixes() {
        scan = JSON.parse(backend.game_lighting_prefixes() || "{}");
        if (scan.auto_installed && scan.auto_installed.length) {
            say(true, "Installed automatically in " + scan.auto_installed.length + " new game" + (scan.auto_installed.length === 1 ? "" : "s") + ".");
        }
    }
    function say(ok, text) {
        messageOk = ok;
        message = text;
    }
    function apply(ids, install) {
        const r = JSON.parse((install ? backend.game_lighting_install(JSON.stringify(ids)) : backend.game_lighting_uninstall(JSON.stringify(ids))) || "{}");
        say(r.ok !== false, r.message || "");
        reloadPrefixes();
    }
    function stateLabel(s) {
        return ({
            installed: "Installed",
            not_installed: "Not installed",
            outdated: "Update available",
            partial: "Incomplete",
            foreign: "Another library is installed",
            invalid: "Not a Wine prefix"
        })[s] || s;
    }
    function sourceLabel(s) {
        return ({ steam: "Steam", lutris: "Lutris", heroic: "Heroic", bottles: "Bottles", custom: "Added by you" })[s] || s;
    }

    Component.onCompleted: {
        reloadStatus();
        reloadPrefixes();
    }
    onVisibleChanged: if (visible) {
        reloadStatus();
        reloadPrefixes();
    }
    Connections {
        target: page.backend
        function onGame_lighting_changed(status) {
            const s = JSON.parse(status || "{}");
            page.gl = Object.assign({}, page.gl, { available: true, running: true, enabled: s.enabled !== undefined ? s.enabled : page.gl.enabled, status: s });
        }
        function onGame_lighting_tested(ok, message) {
            page.testing = false;
            page.testOk = ok;
            page.testResult = ok ? message : "Test failed: " + message;
        }
    }
    // The signal keeps the status live; this is a slow safety net (e.g. after a daemon restart).
    Timer {
        interval: 5000
        repeat: true
        running: page.visible
        onTriggered: page.reloadStatus()
    }

    FolderDialog {
        id: folderPicker
        title: "Choose a Wine prefix (the folder with drive_c)"
        onAccepted: {
            const err = page.backend.game_lighting_add_prefix(selectedFolder.toString());
            if (err !== "") {
                page.say(false, err);
            } else {
                page.say(true, "Prefix added.");
                page.reloadPrefixes();
            }
        }
    }

    QQC2.ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            width: Math.min(page.width - 80, 1000)
            x: Math.max(40, (page.width - width) / 2)
            spacing: 14

            Item {
                Layout.preferredHeight: 10
            }

            // ---- master switch + live status
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: statusCol.implicitHeight + 40
                color: Theme.card
                border.color: page.controlling && page.gl.enabled ? Theme.accent : Theme.borderSoft
                radius: 2

                RowLayout {
                    anchors.fill: parent
                    anchors.margins: 20
                    spacing: 18

                    Rectangle {
                        Layout.alignment: Qt.AlignTop
                        Layout.preferredWidth: 56
                        Layout.preferredHeight: 56
                        radius: 28
                        color: page.gl.available && page.gl.enabled ? Theme.accent : Theme.panelRaised
                        AppGlyph {
                            anchors.centerIn: parent
                            width: 32
                            height: 32
                            kind: "gamelight"
                            colour: page.gl.available && page.gl.enabled ? Theme.accentText : Theme.textDim
                        }
                    }
                    ColumnLayout {
                        id: statusCol
                        Layout.fillWidth: true
                        spacing: 6
                        RowLayout {
                            spacing: 8
                            Text {
                                text: "GAME LIGHTING"
                                color: Theme.text
                                font.family: Theme.font
                                font.pixelSize: Theme.small
                                font.letterSpacing: 1
                                font.bold: true
                            }
                            HelpButton {
                                size: 16
                                text: "Games that support Razer Chroma can colour your keyboard, mouse and other devices while they run, for example health bars, ammo or alerts. Tsuba runs the local lighting service those games look for. Windows games in Wine or Proton also need Tsuba's bridge library inside their prefix: install it below. When the game closes, your own lighting comes back."
                            }
                        }
                        Text {
                            objectName: "glHeadline"
                            Layout.fillWidth: true
                            text: page.headline
                            // Game- or file-system-supplied text: never interpreted as markup.
                            textFormat: Text.PlainText
                            color: page.controlling && page.gl.enabled ? Theme.accent : Theme.textBright
                            font.family: Theme.font
                            font.pixelSize: Theme.heading
                            wrapMode: Text.Wrap
                        }
                        Text {
                            Layout.fillWidth: true
                            text: page.subline
                            // Game- or file-system-supplied text: never interpreted as markup.
                            textFormat: Text.PlainText
                            color: Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                            wrapMode: Text.Wrap
                        }
                        // Who controls each device's lighting right now.
                        Flow {
                            Layout.fillWidth: true
                            Layout.topMargin: 4
                            spacing: 14
                            visible: page.gl.available
                            Repeater {
                                model: JSON.parse(page.backend.devices || "[]")
                                delegate: Row {
                                    required property var modelData
                                    spacing: 8
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: modelData.name.replace(" (wired)", "")
                                        textFormat: Text.PlainText
                                        color: Theme.textDim
                                        font.family: Theme.font
                                        font.pixelSize: Theme.small
                                    }
                                    LightingOwnerChip {
                                        objectName: "ownerChip" + modelData.pid
                                        backend: page.backend
                                        pid: modelData.pid
                                    }
                                }
                            }
                        }
                        RowLayout {
                            Layout.topMargin: 6
                            spacing: 10
                            SynButton {
                                objectName: "glTestButton"
                                text: page.testing ? "Testing…" : "Test"
                                enabled: page.gl.available && page.gl.enabled && !!(page.gl.status && page.gl.status.port) && !page.testing
                                onClicked: {
                                    page.testing = true;
                                    page.testResult = "";
                                    page.backend.game_lighting_test();
                                }
                            }
                            HelpButton {
                                size: 16
                                text: "Sends a moving rainbow to your keyboard and mouse for three seconds, the same way a game would, then gives the lighting back."
                            }
                            Text {
                                Layout.fillWidth: true
                                text: page.testResult
                                // Game- or file-system-supplied text: never interpreted as markup.
                                textFormat: Text.PlainText
                                color: page.testOk ? Theme.textDim : Theme.danger
                                font.family: Theme.font
                                font.pixelSize: Theme.small
                                elide: Text.ElideRight
                            }
                        }
                    }
                    SynToggle {
                        objectName: "glToggle"
                        Layout.alignment: Qt.AlignTop
                        enabled: page.gl.available
                        checked: page.gl.enabled
                        onToggled: on => {
                            if (page.backend.set_game_lighting(on)) {
                                page.gl = Object.assign({}, page.gl, { enabled: on });
                            }
                            page.reloadStatus();
                        }
                    }
                }
            }

            // ---- websites
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                SynCheckBox {
                    objectName: "glWeb"
                    text: "Allow websites to control lighting"
                    enabled: page.gl.available
                    checked: page.gl.web === true
                    onToggled: {
                        if (page.backend.set_game_lighting_web(checked)) {
                            page.gl = Object.assign({}, page.gl, { web: checked });
                        } else {
                            page.say(false, "Couldn't change the setting.");
                        }
                        page.reloadStatus();
                    }
                }
                HelpButton {
                    size: 16
                    text: "Some lighting apps and demos run in a web browser and use the same local lighting service as games. They only work with this on. While it's on, any website you have open could change your lighting, so leave it off unless you use such an app. Games don't need it."
                }
                Item {
                    Layout.fillWidth: true
                }
            }

            // ---- applications allowed to control the lighting
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 10
                spacing: 10
                SectionLabel {
                    caption: i18n("Applications")
                }
                HelpButton {
                    size: 16
                    text: i18n("Every program that connects to Tsuba's lighting service is listed here the first time it does. Block one to stop it from ever controlling your lighting (a blocked program that is running is disconnected at once). Programs are told apart by their process name.")
                }
                Item {
                    Layout.fillWidth: true
                }
            }
            SynCheckBox {
                objectName: "glOnlyAllowed"
                text: i18n("Only allow applications I have approved")
                enabled: page.gl.available
                checked: page.rules.only_allowed === true
                onToggled: page.setOnlyAllowed(checked)
            }
            Text {
                visible: page.rules.apps.length === 0
                Layout.fillWidth: true
                text: i18n("No program has connected yet. When a game does, it appears here so you can allow or block it.")
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: Theme.small
                wrapMode: Text.Wrap
            }
            Repeater {
                model: page.rules.apps
                delegate: Rectangle {
                    required property var modelData
                    objectName: "appRow_" + modelData.key
                    Layout.fillWidth: true
                    Layout.preferredHeight: 52
                    color: Theme.card
                    border.color: Theme.borderSoft
                    radius: 2
                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 16
                        anchors.rightMargin: 12
                        spacing: 12
                        Column {
                            Layout.fillWidth: true
                            Text {
                                width: parent.width
                                // Names come from the program: never interpreted as markup.
                                textFormat: Text.PlainText
                                text: modelData.name || modelData.exe || modelData.key
                                color: modelData.allowed ? Theme.text : Theme.textDim
                                font.family: Theme.font
                                font.pixelSize: Theme.fontSize
                                elide: Text.ElideRight
                            }
                            Text {
                                width: parent.width
                                textFormat: Text.PlainText
                                text: (modelData.exe && modelData.exe !== modelData.name ? modelData.exe + " · " : "") + (modelData.connected ? i18n("connected now") : (modelData.allowed ? i18n("allowed") : i18n("blocked")))
                                color: modelData.connected ? Theme.accent : Theme.textDim
                                font.family: Theme.font
                                font.pixelSize: 11
                                elide: Text.ElideRight
                            }
                        }
                        ModeTabs {
                            objectName: "appRule_" + modelData.key
                            minTabWidth: 78
                            model: [i18n("Default"), i18n("Allow"), i18n("Block")]
                            currentIndex: ["default", "allow", "block"].indexOf(modelData.rule)
                            onActivated: i => page.setRule(modelData.key, ["default", "allow", "block"][i])
                        }
                    }
                }
            }

            // ---- games / prefixes
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 10
                spacing: 10
                SectionLabel {
                    caption: "Windows games (Wine and Proton)"
                }
                HelpButton {
                    size: 16
                    text: "Each Wine or Proton game has its own Windows folder (a prefix). Installing copies Tsuba's bridge library into it so the game finds a lighting service. Tsuba never replaces a library it didn't install, and removing takes out only its own files. Restart a running game after installing. Linux-native games don't need this."
                }
                Item {
                    Layout.fillWidth: true
                }
                SynButton {
                    text: "Add a prefix…"
                    onClicked: folderPicker.open()
                }
                SynButton {
                    text: "Rescan"
                    implicitWidth: 84
                    onClicked: page.reloadPrefixes()
                }
                SynButton {
                    objectName: "glInstallAll"
                    primary: true
                    text: "Install for all"
                    enabled: page.scan.dll_dir !== "" && page.scan.prefixes.some(p => p.state === "not_installed" || p.state === "outdated" || p.state === "partial")
                    onClicked: page.apply(["all"], true)
                }
            }
            SynCheckBox {
                objectName: "glAuto"
                text: "Install automatically for new games"
                checked: page.scan.auto
                onToggled: {
                    if (!page.backend.set_game_lighting_auto(checked)) {
                        page.say(false, "Couldn't save the setting.");
                    }
                    page.reloadPrefixes();
                }
            }
            Text {
                visible: page.scan.dll_dir === ""
                Layout.fillWidth: true
                text: "The bridge library isn't installed on this computer (Tsuba's package puts it in /usr/share/tsuba/game-lighting), so it can't be added to games yet."
                color: Theme.warn
                font.family: Theme.font
                font.pixelSize: Theme.small
                wrapMode: Text.Wrap
            }
            Text {
                objectName: "glMessage"
                visible: page.message !== ""
                Layout.fillWidth: true
                text: page.message
                // Game- or file-system-supplied text: never interpreted as markup.
                textFormat: Text.PlainText
                color: page.messageOk ? Theme.accent : Theme.danger
                font.family: Theme.font
                font.pixelSize: Theme.small
                wrapMode: Text.Wrap
            }

            Repeater {
                model: page.scan.prefixes
                delegate: Rectangle {
                    id: prow
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.preferredHeight: 64
                    color: rh.hovered ? Theme.panelRaised : Theme.card
                    border.color: Theme.borderSoft
                    readonly property bool ours: ["installed", "outdated", "partial"].indexOf(modelData.state) >= 0
                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 12
                        anchors.rightMargin: 12
                        spacing: 12
                        Rectangle {
                            Layout.preferredWidth: 34
                            Layout.preferredHeight: 48
                            color: Theme.panelRaised
                            Image {
                                anchors.fill: parent
                                visible: !!modelData.art
                                source: modelData.art || ""
                                fillMode: Image.PreserveAspectCrop
                                sourceSize.width: 68
                                sourceSize.height: 96
                                asynchronous: true
                            }
                            AppGlyph {
                                visible: !modelData.art
                                anchors.centerIn: parent
                                width: 20
                                height: 20
                                kind: "links"
                                colour: Theme.textDim
                            }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2
                            Text {
                                Layout.fillWidth: true
                                text: modelData.name
                                // Game- or file-system-supplied text: never interpreted as markup.
                                textFormat: Text.PlainText
                                color: Theme.textBright
                                font.family: Theme.font
                                font.pixelSize: Theme.fontSize
                                elide: Text.ElideRight
                            }
                            Text {
                                Layout.fillWidth: true
                                text: page.sourceLabel(modelData.source) + " · " + modelData.path
                                // Game- or file-system-supplied text: never interpreted as markup.
                                textFormat: Text.PlainText
                                color: Theme.textDim
                                font.family: Theme.font
                                font.pixelSize: 11
                                elide: Text.ElideMiddle
                            }
                        }
                        Text {
                            text: page.stateLabel(modelData.state)
                            color: modelData.state === "installed" ? Theme.accent : (modelData.state === "not_installed" ? Theme.textDim : Theme.warn)
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                        SynButton {
                            visible: modelData.state === "outdated" || modelData.state === "partial"
                            text: modelData.state === "outdated" ? "Update" : "Repair"
                            implicitWidth: 84
                            enabled: page.scan.dll_dir !== ""
                            onClicked: page.apply([modelData.id], true)
                        }
                        SynButton {
                            visible: modelData.state !== "invalid"
                            primary: !prow.ours && modelData.state !== "foreign"
                            text: prow.ours ? "Remove" : "Install"
                            implicitWidth: 96
                            enabled: prow.ours || (modelData.state === "not_installed" && page.scan.dll_dir !== "")
                            onClicked: page.apply([modelData.id], !prow.ours)
                        }
                    }
                    HoverHandler {
                        id: rh
                    }
                }
            }
            Text {
                visible: page.scan.prefixes.length === 0
                Layout.fillWidth: true
                text: "No Wine or Proton games found. Steam games appear here after their first launch with Proton; use \"Add a prefix…\" for others."
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: Theme.fontSize
                wrapMode: Text.Wrap
            }
            Item {
                Layout.preferredHeight: 24
            }
        }
    }
}
