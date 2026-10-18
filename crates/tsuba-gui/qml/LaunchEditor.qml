import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// Launch: a ready-made program (calculator, terminal, file manager…), any installed
// application from the desktop's menu, your own command, or a website.
ColumnLayout {
    id: ed

    required property var panel
    property int mode: 0   // 0 program, 1 website
    property var presets: []
    property var apps: []
    property string filter: ""
    readonly property string terminal: (presets.find(p => p.id === "terminal") || { command: "" }).command
    readonly property var shown: apps.filter(a => filter === "" || a.name.toLowerCase().indexOf(filter.toLowerCase()) >= 0 || a.command.toLowerCase().indexOf(filter.toLowerCase()) >= 0).slice(0, 60)

    spacing: 8

    function load() {
        presets = panel.lp.launchPresets();
        apps = panel.lp.desktopApps();
    }
    // Read the lists (applications, audio devices, layouts) only once the editor is shown.
    property bool loaded: false
    function ensureLoaded() {
        if (!loaded) {
            loaded = true;
            load();
        }
    }
    onVisibleChanged: if (visible) ensureLoaded()
    Component.onCompleted: if (visible) ensureLoaded()

    function commandFor(app) {
        // Programs that need a terminal run in one.
        return app.terminal && terminal !== "" ? terminal + " -e " + app.command : app.command;
    }
    function stageCommand(cmd, label) {
        const c = cmd.trim();
        if (c === "") {
            return;
        }
        panel.stage("launch:" + c, label || i18n("Launch %1", c), () => panel.assignAction({ type: "launch", command: c }));
    }
    function stageUrl(text) {
        const t = text.trim();
        if (t === "") {
            return;
        }
        const url = (/^[a-z]+:/i.test(t) ? "" : "https://") + t;
        panel.stage("url:" + url, i18n("Open %1", url), () => panel.assignAction({ type: "url", url: url }));
    }

    ModeTabs {
        id: kind
        minTabWidth: 110
        model: [i18n("Program"), i18n("Website")]
        currentIndex: ed.mode
        onActivated: i => ed.mode = i
    }

    // ---- program
    ColumnLayout {
        visible: ed.mode === 0
        Layout.fillWidth: true
        spacing: 6
        SectionHeading { text: i18n("READY-MADE") }
        Repeater {
            model: ed.presets
            delegate: OptionRow {
                required property var modelData
                objectName: "preset_" + modelData.id
                enabled: modelData.command !== ""
                opacity: enabled ? 1 : 0.45
                label: modelData.label + (modelData.command !== "" ? "  ·  " + modelData.command : i18n("  (not installed)"))
                chosen: ed.panel.stagedId === "launch:" + modelData.command
                onPicked: {
                    cmdField.text = modelData.command;
                    ed.stageCommand(modelData.command, i18n("Launch %1", modelData.label));
                }
            }
        }
        SectionHeading { text: i18n("INSTALLED APPLICATIONS") }
        QQC2.TextField {
            id: search
            objectName: "appSearch"
            Layout.fillWidth: true
            placeholderText: i18n("Search applications")
            onTextEdited: ed.filter = text
        }
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(Math.max(ed.shown.length, 1) * 30, 180)
            color: Theme.bg
            border.color: Theme.borderSoft
            clip: true
            ListView {
                id: appList
                objectName: "appList"
                anchors.fill: parent
                anchors.margins: 1
                model: ed.shown
                boundsBehavior: Flickable.StopAtBounds
                QQC2.ScrollBar.vertical: QQC2.ScrollBar {}
                delegate: Rectangle {
                    required property var modelData
                    width: appList.width
                    height: 30
                    color: ah.hovered ? "#14ffffff" : "transparent"
                    Text {
                        anchors.left: parent.left
                        anchors.leftMargin: 10
                        anchors.right: parent.right
                        anchors.rightMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData.name + (modelData.terminal ? i18n("  (terminal)") : "")
                        color: ed.panel.stagedId === "launch:" + ed.commandFor(modelData) ? Theme.accent : Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        elide: Text.ElideRight
                    }
                    HoverHandler {
                        id: ah
                        cursorShape: Qt.PointingHandCursor
                    }
                    TapHandler {
                        onTapped: {
                            const c = ed.commandFor(modelData);
                            cmdField.text = c;
                            ed.stageCommand(c, i18n("Launch %1", modelData.name));
                        }
                    }
                }
            }
            Text {
                anchors.centerIn: parent
                visible: ed.shown.length === 0
                text: ed.apps.length === 0 ? i18n("No applications found") : i18n("Nothing matches")
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: Theme.small
            }
        }
        SectionHeading { text: i18n("OR TYPE A COMMAND") }
        QQC2.TextField {
            id: cmdField
            objectName: "launchCommand"
            Layout.fillWidth: true
            placeholderText: i18n("Command, e.g. firefox or steam")
            onTextEdited: ed.stageCommand(text)
        }
    }

    // ---- website
    ColumnLayout {
        visible: ed.mode === 1
        Layout.fillWidth: true
        spacing: 6
        QQC2.TextField {
            id: urlField
            objectName: "launchUrl"
            Layout.fillWidth: true
            placeholderText: i18n("https://…")
            onTextEdited: ed.stageUrl(text)
        }
    }
    Text {
        Layout.fillWidth: true
        text: i18n("Runs on your PC while Tsuba's service is running.")
        color: Theme.textMuted
        font.family: Theme.font
        font.pixelSize: 11
        wrapMode: Text.Wrap
    }
}
