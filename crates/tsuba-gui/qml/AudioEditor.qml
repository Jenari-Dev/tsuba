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
    Component.onCompleted: load()

    function stageOp(id, label, op) {
        panel.stage("audio:" + id, label, () => panel.assignAction({ type: "audio", op: op }));
    }

    SectionHeading { text: "SPEAKERS" }
    Repeater {
        model: [["Volume up", "volume_up"], ["Volume down", "volume_down"], ["Mute / unmute", "mute_toggle"], ["Next audio output", "next_output"], ["Previous audio output", "previous_output"]]
        delegate: OptionRow {
            required property var modelData
            label: modelData[0]
            chosen: ed.panel.stagedId === "audio:" + modelData[1]
            onPicked: ed.stageOp(modelData[1], modelData[0], modelData[1])
        }
    }
    SectionHeading { text: "MICROPHONE" }
    Repeater {
        model: [["Microphone volume up", "mic_volume_up"], ["Microphone volume down", "mic_volume_down"], ["Mic mute / unmute", "mic_mute_toggle"]]
        delegate: OptionRow {
            required property var modelData
            label: modelData[0]
            chosen: ed.panel.stagedId === "audio:" + modelData[1]
            onPicked: ed.stageOp(modelData[1], modelData[0], modelData[1])
        }
    }

    SectionHeading { text: "SWITCH TO AN OUTPUT" }
    Text {
        visible: ed.sinks.length === 0
        text: "No audio outputs found."
        color: Theme.textDim
        font.family: Theme.font
        font.pixelSize: Theme.small
    }
    SynDropdown {
        id: sinkPick
        objectName: "sinkPick"
        visible: ed.sinks.length > 0
        Layout.fillWidth: true
        model: ed.sinks.map(s => s.description + (s.default ? "  (now)" : ""))
        currentIndex: ed.sinkIdx
        onActivated: i => ed.sinkIdx = i
    }
    SynButton {
        objectName: "setOutput"
        visible: ed.sinks.length > 0
        text: "Use this output"
        enabled: ed.sinks.length > 0
        onClicked: {
            const s = ed.sinks[ed.sinkIdx];
            ed.panel.stage("audio:out:" + s.name, "Output: " + s.description, () => ed.panel.assignAction({ type: "audio", op: { set_default_output: { name: s.name, label: s.description } } }));
        }
    }
    SectionHeading { text: "SWITCH TO AN INPUT" }
    Text {
        visible: ed.sources.length === 0
        text: "No audio inputs found."
        color: Theme.textDim
        font.family: Theme.font
        font.pixelSize: Theme.small
    }
    SynDropdown {
        id: sourcePick
        objectName: "sourcePick"
        visible: ed.sources.length > 0
        Layout.fillWidth: true
        model: ed.sources.map(s => s.description + (s.default ? "  (now)" : ""))
        currentIndex: ed.sourceIdx
        onActivated: i => ed.sourceIdx = i
    }
    SynButton {
        objectName: "setInput"
        visible: ed.sources.length > 0
        text: "Use this input"
        onClicked: {
            const s = ed.sources[ed.sourceIdx];
            ed.panel.stage("audio:in:" + s.name, "Input: " + s.description, () => ed.panel.assignAction({ type: "audio", op: { set_default_input: { name: s.name, label: s.description } } }));
        }
    }
}
