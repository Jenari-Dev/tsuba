import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// Desktop sound from a key: volume and mute for the speakers and the microphone, stepping
// through outputs, or switching straight to a chosen output or input.
ColumnLayout {
    id: ed

    required property var panel
    property var sinks: []
    property var sources: []
    property int sinkIdx: 0
    property int sourceIdx: 0

    spacing: 6

    function load() {
        const d = panel.lp.json("ListAudioDevices", [], { sinks: [], sources: [] });
        sinks = d.sinks;
        sources = d.sources;
        sinkIdx = Math.max(0, sinks.findIndex(s => s.default));
        sourceIdx = Math.max(0, sources.findIndex(s => s.default));
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

    function stageOp(id, label, op) {
        panel.stage("audio:" + id, label, () => panel.assignAction({ type: "audio", op: op }));
    }

    SectionHeading { text: i18n("SPEAKERS") }
    Repeater {
        model: [[i18n("Volume up"), "volume_up"], [i18n("Volume down"), "volume_down"], [i18n("Mute / unmute"), "mute_toggle"], [i18n("Next audio output"), "next_output"], [i18n("Previous audio output"), "previous_output"]]
        delegate: OptionRow {
            required property var modelData
            label: modelData[0]
            chosen: ed.panel.stagedId === "audio:" + modelData[1]
            onPicked: ed.stageOp(modelData[1], modelData[0], modelData[1])
        }
    }
    SectionHeading { text: i18n("MICROPHONE") }
    Repeater {
        model: [[i18n("Microphone volume up"), "mic_volume_up"], [i18n("Microphone volume down"), "mic_volume_down"], [i18n("Mic mute / unmute"), "mic_mute_toggle"]]
        delegate: OptionRow {
            required property var modelData
            label: modelData[0]
            chosen: ed.panel.stagedId === "audio:" + modelData[1]
            onPicked: ed.stageOp(modelData[1], modelData[0], modelData[1])
        }
    }

    SectionHeading { text: i18n("SWITCH TO AN OUTPUT") }
    Text {
        visible: ed.sinks.length === 0
        text: i18n("No audio outputs found.")
        color: Theme.textDim
        font.family: Theme.font
        font.pixelSize: Theme.small
    }
    SynDropdown {
        id: sinkPick
        objectName: "sinkPick"
        visible: ed.sinks.length > 0
        Layout.fillWidth: true
        model: ed.sinks.map(s => s.description + (s.default ? i18n("  (now)") : ""))
        currentIndex: ed.sinkIdx
        onActivated: i => ed.sinkIdx = i
    }
    SynButton {
        objectName: "setOutput"
        visible: ed.sinks.length > 0
        text: i18n("Use this output")
        enabled: ed.sinks.length > 0
        onClicked: {
            const s = ed.sinks[ed.sinkIdx];
            ed.panel.stage("audio:out:" + s.name, i18n("Output: %1", s.description), () => ed.panel.assignAction({ type: "audio", op: { set_default_output: { name: s.name, label: s.description } } }));
        }
    }
    SectionHeading { text: i18n("SWITCH TO AN INPUT") }
    Text {
        visible: ed.sources.length === 0
        text: i18n("No audio inputs found.")
        color: Theme.textDim
        font.family: Theme.font
        font.pixelSize: Theme.small
    }
    SynDropdown {
        id: sourcePick
        objectName: "sourcePick"
        visible: ed.sources.length > 0
        Layout.fillWidth: true
        model: ed.sources.map(s => s.description + (s.default ? i18n("  (now)") : ""))
        currentIndex: ed.sourceIdx
        onActivated: i => ed.sourceIdx = i
    }
    SynButton {
        objectName: "setInput"
        visible: ed.sources.length > 0
        text: i18n("Use this input")
        onClicked: {
            const s = ed.sources[ed.sourceIdx];
            ed.panel.stage("audio:in:" + s.name, i18n("Input: %1", s.description), () => ed.panel.assignAction({ type: "audio", op: { set_default_input: { name: s.name, label: s.description } } }));
        }
    }
}
